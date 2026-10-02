//! Draws [`gource_draw::DrawList`]s with Bevy.
//!
//! Each batch of the frame's draw list becomes one `Mesh2d` entity from a
//! reusable pool. Painter's order is kept through the entities' `z`
//! (the 2D transparent phase sorts by it); the vertex shaders ignore the
//! transform and map screen pixels straight to clip space.
//!
//! Colour handling matches the original OpenGL renderer: textures are
//! uploaded as non-sRGB `Rgba8Unorm`, shaders output raw values, and the
//! camera composites in sRGB space (`CompositingSpace::Srgb`, which must be
//! added to the camera explicitly), so blending happens on gamma-encoded
//! values.

use std::collections::{HashMap, HashSet};

use bevy::{
    asset::{RenderAssetUsages, embedded_asset},
    camera::visibility::NoFrustumCulling,
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    prelude::*,
    reflect::TypePath,
    render::render_resource::{
        AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
        RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension, TextureFormat,
    },
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin},
};
use gource_draw::{Batch, DrawList, Filter, Material, Texture, TextureId, TextureStore, Wrap};

const SCENE_SHADER: &str = "embedded://gource/render/scene.wgsl";
const BLOOM_SHADER: &str = "embedded://gource/render/bloom.wgsl";

/// `glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA)` (applied to alpha too).
pub const GL_ALPHA_BLEND: BlendState = BlendState {
    color: BlendComponent {
        src_factor: BlendFactor::SrcAlpha,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    },
    alpha: BlendComponent {
        src_factor: BlendFactor::SrcAlpha,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    },
};

/// `glBlendFunc(GL_ONE, GL_ONE)`.
pub const GL_ADDITIVE_BLEND: BlendState = BlendState {
    color: BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
    alpha: BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
};

fn set_blend(descriptor: &mut RenderPipelineDescriptor, blend: BlendState) {
    if let Some(fragment) = descriptor.fragment.as_mut() {
        for target in fragment.targets.iter_mut().flatten() {
            target.blend = Some(blend);
        }
    }
}

/// [`Material::Alpha`]: texture × vertex colour, alpha blended.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct SceneMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
}

impl Material2d for SceneMaterial {
    fn vertex_shader() -> ShaderRef {
        SCENE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        SCENE_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, GL_ALPHA_BLEND);
        Ok(())
    }
}

/// [`Material::Bloom`]: the additive glow around directories.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct BloomMaterial {
    #[uniform(0)]
    pub params: Vec4,
}

impl Material2d for BloomMaterial {
    fn vertex_shader() -> ShaderRef {
        BLOOM_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        BLOOM_SHADER.into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, GL_ADDITIVE_BLEND);
        Ok(())
    }
}

/// The draw list to show this frame. Systems in [`DrawListSet::Produce`]
/// fill it; [`DrawListSet::Upload`] mirrors it to entities.
#[derive(Resource, Default)]
pub struct FrameDrawList(pub DrawList);

/// Supplies the texture store the frame's draw list refers to.
///
/// The simulation owns its [`gource_draw::Gfx`]; the frontend installs a
/// closure-free accessor by copying textures through this resource each
/// frame (only changed textures are converted).
#[derive(Resource, Default)]
pub struct FrameTextures {
    /// Textures that changed (or appeared) since the last upload.
    pub changed: Vec<(TextureId, Texture)>,
    /// Ids of all live textures, when known (used to drop removed ones).
    pub live: Option<HashSet<TextureId>>,
}

impl FrameTextures {
    /// Queue every texture in `store` whose version differs from what the
    /// GPU has.
    pub fn collect(&mut self, store: &TextureStore, gpu: &GpuTextures) {
        let mut live = HashSet::new();
        for (id, texture) in store.iter() {
            live.insert(id);
            if gpu.version(id) != Some(texture.version) {
                self.changed.push((id, texture.clone()));
            }
        }
        self.live = Some(live);
    }
}

/// System sets, in order.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DrawListSet {
    /// Produce [`FrameDrawList`] / [`FrameTextures`].
    Produce,
    /// Upload them to Bevy assets and entities.
    Upload,
}

struct GpuTexture {
    image: Handle<Image>,
    material: Handle<SceneMaterial>,
    version: u64,
}

/// Textures mirrored to the GPU, keyed by [`TextureId`].
#[derive(Resource, Default)]
pub struct GpuTextures {
    entries: HashMap<TextureId, GpuTexture>,
}

impl GpuTextures {
    pub fn version(&self, id: TextureId) -> Option<u64> {
        self.entries.get(&id).map(|e| e.version)
    }

    fn material(&self, id: TextureId) -> Option<Handle<SceneMaterial>> {
        self.entries
            .get(&id)
            .or_else(|| self.entries.get(&TextureId::WHITE))
            .map(|e| e.material.clone())
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum SlotMaterial {
    None,
    Scene(TextureId),
    Bloom,
}

struct Slot {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: SlotMaterial,
    visible: bool,
}

/// Reusable batch entities (one per batch of the largest frame so far).
#[derive(Resource, Default)]
pub struct BatchPool {
    slots: Vec<Slot>,
    bloom: Handle<BloomMaterial>,
    /// Clear colour last applied to the camera.
    clear_colour: Option<gource_core::Vec4>,
}

impl BatchPool {
    /// Entities of all pooled batch slots, in draw order.
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().map(|s| s.entity)
    }

    /// Number of slots currently shown.
    pub fn visible(&self) -> usize {
        self.slots.iter().filter(|s| s.visible).count()
    }
}

/// Marks the camera that shows the draw list.
#[derive(Component)]
pub struct DrawListCamera;

/// Renders [`FrameDrawList`] every frame.
pub struct DrawListPlugin {
    /// 4x multisampling (`--multi-sampling`).
    pub multisample: bool,
}

impl Plugin for DrawListPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "scene.wgsl");
        embedded_asset!(app, "bloom.wgsl");

        let msaa = if self.multisample {
            Msaa::Sample4
        } else {
            Msaa::Off
        };

        app.add_plugins((
            Material2dPlugin::<SceneMaterial>::default(),
            Material2dPlugin::<BloomMaterial>::default(),
        ))
        .init_resource::<FrameDrawList>()
        .init_resource::<FrameTextures>()
        .init_resource::<GpuTextures>()
        .init_resource::<BatchPool>()
        .configure_sets(Update, (DrawListSet::Produce, DrawListSet::Upload).chain())
        .add_systems(
            Startup,
            move |commands: Commands,
                  pool: ResMut<BatchPool>,
                  blooms: ResMut<Assets<BloomMaterial>>| {
                spawn_camera(commands, pool, blooms, msaa);
            },
        )
        .add_systems(
            Update,
            (upload_textures, upload_batches)
                .chain()
                .in_set(DrawListSet::Upload),
        );
    }
}

/// Spawn the draw list camera and create the shared bloom material.
pub fn spawn_camera(
    mut commands: Commands,
    mut pool: ResMut<BatchPool>,
    mut blooms: ResMut<Assets<BloomMaterial>>,
    msaa: Msaa,
) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        // Blend gamma-encoded values like the OpenGL renderer: the main
        // texture is `Rgba8Unorm`, the shaders' raw output is stored as is
        // and the final blit decodes it for the sRGB surface. Without this
        // component Bevy renders into an sRGB target, treating shader output
        // as linear light.
        CompositingSpace::Srgb,
        msaa,
        Tonemapping::None,
        DebandDither::Disabled,
        DrawListCamera,
    ));
    pool.bloom = blooms.add(BloomMaterial::default());
}

/// Convert a CPU texture (with its mip chain) to a Bevy image.
pub fn texture_to_image(texture: &Texture) -> Image {
    let size = Extent3d {
        width: texture.width.max(1),
        height: texture.height.max(1),
        depth_or_array_layers: 1,
    };
    let level0 = texture
        .levels
        .first()
        .cloned()
        .unwrap_or_else(|| vec![255; 4]);
    let mut image = Image::new(
        size,
        TextureDimension::D2,
        level0,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::default(),
    );
    if texture.levels.len() > 1 {
        let data: Vec<u8> = texture.levels.concat();
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = texture.levels.len() as u32;
    }
    let address_mode = match texture.options.wrap {
        Wrap::Clamp => ImageAddressMode::ClampToEdge,
        Wrap::Repeat => ImageAddressMode::Repeat,
    };
    let filter = match texture.options.filter {
        Filter::Nearest => ImageFilterMode::Nearest,
        Filter::Linear => ImageFilterMode::Linear,
    };
    let mipmap_filter = if texture.levels.len() > 1 {
        ImageFilterMode::Linear
    } else {
        ImageFilterMode::Nearest
    };
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter,
        ..default()
    });
    image
}

pub fn upload_textures(
    mut frame: ResMut<FrameTextures>,
    mut gpu: ResMut<GpuTextures>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
) {
    for (id, texture) in frame.changed.drain(..) {
        let image = texture_to_image(&texture);
        match gpu.entries.get_mut(&id) {
            Some(entry) => {
                let _ = images.insert(entry.image.id(), image);
                // Touch the material so its bind group picks up the new texture.
                if let Some(mut material) = materials.get_mut(&entry.material) {
                    material.texture = entry.image.clone();
                }
                entry.version = texture.version;
            }
            None => {
                let image = images.add(image);
                let material = materials.add(SceneMaterial {
                    texture: image.clone(),
                });
                gpu.entries.insert(
                    id,
                    GpuTexture {
                        image,
                        material,
                        version: texture.version,
                    },
                );
            }
        }
    }
    if let Some(live) = frame.live.take() {
        gpu.entries.retain(|id, _| live.contains(id));
    }
}

/// Write a batch into a mesh (positions in pixels, z unused).
pub fn write_mesh(mesh: &mut Mesh, batch: &Batch) {
    let positions: Vec<[f32; 3]> = batch
        .vertices
        .iter()
        .map(|v| [v.pos.x, v.pos.y, 0.0])
        .collect();
    let uvs: Vec<[f32; 2]> = batch.vertices.iter().map(|v| [v.uv.x, v.uv.y]).collect();
    let colours: Vec<[f32; 4]> = batch.vertices.iter().map(|v| v.colour.to_array()).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    mesh.insert_indices(Indices::U32(batch.indices.clone()));
}

fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));
    mesh
}

#[allow(clippy::too_many_arguments)]
pub fn upload_batches(
    mut commands: Commands,
    frame: Res<FrameDrawList>,
    gpu: Res<GpuTextures>,
    mut pool: ResMut<BatchPool>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cameras: Query<&mut Camera, With<DrawListCamera>>,
) {
    let list = &frame.0;
    let pool = &mut *pool;

    let c = list.clear_colour;
    if pool.clear_colour != Some(c) {
        for mut camera in &mut cameras {
            camera.clear_color = ClearColorConfig::Custom(Color::srgba(c.x, c.y, c.z, c.w));
        }
        pool.clear_colour = Some(c);
    }

    let batches: Vec<&Batch> = list
        .batches
        .iter()
        .filter(|b| !b.indices.is_empty())
        .collect();

    for (i, batch) in batches.iter().enumerate() {
        if i == pool.slots.len() {
            let mesh = meshes.add(empty_mesh());
            let entity = commands
                .spawn((
                    Mesh2d(mesh.clone()),
                    Transform::from_xyz(0.0, 0.0, i as f32),
                    NoFrustumCulling,
                    Visibility::Visible,
                ))
                .id();
            pool.slots.push(Slot {
                entity,
                mesh,
                material: SlotMaterial::None,
                visible: true,
            });
        }
        let slot = &mut pool.slots[i];

        if let Some(mut mesh) = meshes.get_mut(&slot.mesh) {
            write_mesh(&mut mesh, batch);
        }

        let wanted = match batch.material {
            Material::Alpha => SlotMaterial::Scene(batch.texture),
            Material::Bloom => SlotMaterial::Bloom,
        };
        if wanted != slot.material {
            let mut entity = commands.entity(slot.entity);
            match (&slot.material, &wanted) {
                (SlotMaterial::Bloom, SlotMaterial::Scene(_)) => {
                    entity.remove::<MeshMaterial2d<BloomMaterial>>();
                }
                (SlotMaterial::Scene(_), SlotMaterial::Bloom) => {
                    entity.remove::<MeshMaterial2d<SceneMaterial>>();
                }
                _ => {}
            }
            match &wanted {
                SlotMaterial::Scene(texture) => {
                    let Some(material) = gpu.material(*texture) else {
                        // Texture not uploaded yet: skip this batch for now.
                        continue;
                    };
                    entity.insert(MeshMaterial2d(material));
                }
                SlotMaterial::Bloom => {
                    entity.insert(MeshMaterial2d(pool.bloom.clone()));
                }
                SlotMaterial::None => {}
            }
            slot.material = wanted;
        }

        if !slot.visible {
            commands.entity(slot.entity).insert(Visibility::Visible);
            slot.visible = true;
        }
    }

    for slot in pool.slots.iter_mut().skip(batches.len()) {
        if slot.visible {
            commands.entity(slot.entity).insert(Visibility::Hidden);
            slot.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gource_draw::{TextureOptions, Vertex};

    fn texture(levels: Vec<Vec<u8>>, width: u32, height: u32, options: TextureOptions) -> Texture {
        Texture {
            name: "t".into(),
            width,
            height,
            options,
            levels,
            version: 1,
        }
    }

    #[test]
    fn image_keeps_raw_values_and_mips() {
        let level0 = vec![10, 20, 30, 40, 50, 60, 70, 80];
        let level1 = vec![30, 40, 50, 60];
        let tex = texture(vec![level0, level1], 2, 1, TextureOptions::default());
        let image = texture_to_image(&tex);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(image.texture_descriptor.mip_level_count, 2);
        assert_eq!(
            image.data.as_deref(),
            Some(&[10, 20, 30, 40, 50, 60, 70, 80, 30, 40, 50, 60][..])
        );
        match &image.sampler {
            ImageSampler::Descriptor(d) => {
                assert_eq!(d.mipmap_filter, ImageFilterMode::Linear);
                assert_eq!(d.address_mode_u, ImageAddressMode::ClampToEdge);
            }
            other => panic!("unexpected sampler {other:?}"),
        }
    }

    #[test]
    fn plain_repeat_texture_sampler() {
        let options = TextureOptions {
            mipmaps: false,
            wrap: Wrap::Repeat,
            filter: Filter::Nearest,
        };
        let tex = texture(vec![vec![1, 2, 3, 4]], 1, 1, options);
        let image = texture_to_image(&tex);
        assert_eq!(image.texture_descriptor.mip_level_count, 1);
        match &image.sampler {
            ImageSampler::Descriptor(d) => {
                assert_eq!(d.address_mode_v, ImageAddressMode::Repeat);
                assert_eq!(d.mag_filter, ImageFilterMode::Nearest);
                assert_eq!(d.mipmap_filter, ImageFilterMode::Nearest);
            }
            other => panic!("unexpected sampler {other:?}"),
        }
    }

    #[test]
    fn mesh_attributes_from_batch() {
        let mut batch = Batch::new(Material::Alpha, TextureId::WHITE);
        batch.vertices = vec![
            Vertex::new(
                gource_core::Vec2::new(1.0, 2.0),
                gource_core::Vec2::new(0.0, 1.0),
                gource_core::Vec4::new(1.0, 0.5, 0.25, 1.0),
            ),
            Vertex::new(
                gource_core::Vec2::new(3.0, 4.0),
                gource_core::Vec2::new(1.0, 0.0),
                gource_core::Vec4::ONE,
            ),
            Vertex::new(
                gource_core::Vec2::new(5.0, 6.0),
                gource_core::Vec2::ONE,
                gource_core::Vec4::ONE,
            ),
        ];
        batch.indices = vec![0, 1, 2];
        let mut mesh = empty_mesh();
        write_mesh(&mut mesh, &batch);
        assert_eq!(mesh.count_vertices(), 3);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        assert_eq!(pos[1], [3.0, 4.0, 0.0]);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(col)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        assert_eq!(col[0], [1.0, 0.5, 0.25, 1.0]);
        assert_eq!(mesh.indices().map(|i| i.len()), Some(3));
    }
}
