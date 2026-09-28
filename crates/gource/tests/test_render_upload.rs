use bevy::{asset::AssetPlugin, prelude::*, sprite_render::MeshMaterial2d};
use glam::{UVec2, Vec2, Vec4};
use gource::render::{
    BatchPool, BloomMaterial, DrawListCamera, FrameDrawList, FrameTextures, GpuTextures,
    SceneMaterial, spawn_camera, upload_batches, upload_textures,
};
use gource_draw::{Filter, TextureId, TextureOptions, TextureStore, Wrap};

fn create_render_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.init_asset::<Mesh>();
    app.init_asset::<Image>();
    app.init_asset::<SceneMaterial>();
    app.init_asset::<BloomMaterial>();

    app.init_resource::<BatchPool>();
    app.init_resource::<FrameDrawList>();
    app.init_resource::<FrameTextures>();
    app.init_resource::<GpuTextures>();

    app.add_systems(
        Startup,
        |commands: Commands, pool: ResMut<BatchPool>, blooms: ResMut<Assets<BloomMaterial>>| {
            spawn_camera(commands, pool, blooms, Msaa::Off);
        },
    );
    app.add_systems(Update, (upload_textures, upload_batches).chain());

    app
}

#[test]
fn test_render_upload_lifecycle() {
    let mut app = create_render_app();
    app.update(); // run Startup

    // 1. Initially 0 entities in pool
    assert_eq!(app.world().resource::<BatchPool>().visible(), 0);

    // 2. Prepare TextureStore with WHITE and a custom texture
    let mut store = TextureStore::default();
    let custom_id = store.create_rgba(
        "test",
        1,
        1,
        vec![100, 100, 100, 255],
        TextureOptions {
            mipmaps: false,
            wrap: Wrap::Clamp,
            filter: Filter::Nearest,
        },
    );

    // Upload textures
    {
        let world = app.world_mut();
        world.resource_scope(|world, mut frame_tex: Mut<FrameTextures>| {
            let gpu = world.resource::<GpuTextures>();
            frame_tex.collect(&store, gpu);
        });
    }
    app.update();

    assert_eq!(
        app.world()
            .resource::<GpuTextures>()
            .version(TextureId::WHITE),
        Some(1)
    );
    assert_eq!(
        app.world().resource::<GpuTextures>().version(custom_id),
        Some(1)
    );

    // 3. Draw a list with 3 batches: Alpha(custom_id), Alpha(TextureId::WHITE), Bloom
    {
        let mut list = app.world_mut().resource_mut::<FrameDrawList>();
        list.0
            .reset(UVec2::new(800, 600), Vec4::new(0.2, 0.3, 0.4, 1.0));
        // Batch 1: custom texture
        list.0.rect(
            custom_id,
            Vec2::new(10.0, 10.0),
            Vec2::new(50.0, 50.0),
            Vec4::ONE,
        );
        // Batch 2: solid rect (uses WHITE texture)
        list.0.solid_rect(
            Vec2::new(100.0, 10.0),
            Vec2::new(50.0, 50.0),
            Vec4::new(1.0, 0.0, 0.0, 1.0),
        );
        // Batch 3: bloom
        list.0.bloom(Vec2::new(200.0, 200.0), 30.0, Vec4::ONE);
    }

    app.update();

    {
        let pool = app.world().resource::<BatchPool>();
        assert_eq!(pool.visible(), 3);
        let entities: Vec<Entity> = pool.entities().collect();
        assert_eq!(entities.len(), 3);

        // Check camera clear colour was set
        let mut cam_query = app
            .world_mut()
            .query_filtered::<&Camera, With<DrawListCamera>>();
        let cam = cam_query.single(app.world()).unwrap();
        match cam.clear_color {
            ClearColorConfig::Custom(c) => {
                let srgba = c.to_srgba();
                assert!((srgba.red - 0.2).abs() < 1e-4);
                assert!((srgba.green - 0.3).abs() < 1e-4);
                assert!((srgba.blue - 0.4).abs() < 1e-4);
            }
            _ => panic!("expected custom clear colour"),
        }

        // Entity 0 and 1 have SceneMaterial, entity 2 has BloomMaterial
        assert!(
            app.world()
                .get::<MeshMaterial2d<SceneMaterial>>(entities[0])
                .is_some()
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<SceneMaterial>>(entities[1])
                .is_some()
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<BloomMaterial>>(entities[2])
                .is_some()
        );

        // Check transforms (z is slot index)
        let t0 = app.world().get::<Transform>(entities[0]).unwrap();
        let t1 = app.world().get::<Transform>(entities[1]).unwrap();
        let t2 = app.world().get::<Transform>(entities[2]).unwrap();
        assert_eq!(t0.translation.z, 0.0);
        assert_eq!(t1.translation.z, 1.0);
        assert_eq!(t2.translation.z, 2.0);
    }

    // 4. Switch material in slot 0 to Bloom, and shrink batches from 3 to 1
    {
        let mut list = app.world_mut().resource_mut::<FrameDrawList>();
        list.0
            .reset(UVec2::new(800, 600), Vec4::new(0.0, 0.0, 0.0, 1.0));
        // Slot 0 now gets bloom!
        list.0.bloom(Vec2::new(50.0, 50.0), 20.0, Vec4::ONE);
    }

    app.update();

    {
        let pool = app.world().resource::<BatchPool>();
        assert_eq!(pool.visible(), 1);
        let entities: Vec<Entity> = pool.entities().collect();
        assert_eq!(entities.len(), 3); // Allocated slots remain 3

        // Slot 0 now has BloomMaterial, not SceneMaterial
        assert!(
            app.world()
                .get::<MeshMaterial2d<BloomMaterial>>(entities[0])
                .is_some()
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<SceneMaterial>>(entities[0])
                .is_none()
        );

        // Slot 1 and 2 are hidden
        let v0 = app.world().get::<Visibility>(entities[0]).unwrap();
        let v1 = app.world().get::<Visibility>(entities[1]).unwrap();
        let v2 = app.world().get::<Visibility>(entities[2]).unwrap();
        assert_eq!(*v0, Visibility::Visible);
        assert_eq!(*v1, Visibility::Hidden);
        assert_eq!(*v2, Visibility::Hidden);
    }

    // 5. Expand back to 2 batches, testing visibility restoration and fallback texture
    {
        let unknown_id = TextureId(9999);
        let mut list = app.world_mut().resource_mut::<FrameDrawList>();
        list.0.reset(UVec2::new(800, 600), Vec4::ZERO);
        // Slot 0 switches back to Scene with unknown texture (falls back to WHITE material)
        list.0
            .rect(unknown_id, Vec2::ZERO, Vec2::splat(10.0), Vec4::ONE);
        // Slot 1 is visible again with bloom
        list.0.bloom(Vec2::splat(10.0), 10.0, Vec4::ONE);
    }

    app.update();

    {
        let pool = app.world().resource::<BatchPool>();
        assert_eq!(pool.visible(), 2);
        let entities: Vec<Entity> = pool.entities().collect();

        // Slot 0 has SceneMaterial (fallen back to WHITE)
        assert!(
            app.world()
                .get::<MeshMaterial2d<SceneMaterial>>(entities[0])
                .is_some()
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<BloomMaterial>>(entities[0])
                .is_none()
        );

        // Slot 1 is visible with BloomMaterial
        assert_eq!(
            *app.world().get::<Visibility>(entities[1]).unwrap(),
            Visibility::Visible
        );
        assert!(
            app.world()
                .get::<MeshMaterial2d<BloomMaterial>>(entities[1])
                .is_some()
        );
    }

    // 6. Texture version update and removal of old textures
    {
        store.update_rgba(custom_id, 0, 0, 1, 1, &[200, 200, 200, 255]);
        let world = app.world_mut();
        world.resource_scope(|world, mut frame_tex: Mut<FrameTextures>| {
            let gpu = world.resource::<GpuTextures>();
            frame_tex.collect(&store, gpu);
        });
    }

    app.update();

    {
        let gpu = app.world().resource::<GpuTextures>();
        assert_eq!(gpu.version(custom_id), Some(2));
    }

    // Now remove custom_id from live set
    {
        let new_store = TextureStore::default(); // only has WHITE
        let world = app.world_mut();
        world.resource_scope(|world, mut frame_tex: Mut<FrameTextures>| {
            let gpu = world.resource::<GpuTextures>();
            frame_tex.collect(&new_store, gpu);
        });
    }

    app.update();

    {
        let gpu = app.world().resource::<GpuTextures>();
        // custom_id is not in new_store so it should have been retained out
        assert_eq!(gpu.version(custom_id), None);
        assert_eq!(gpu.version(TextureId::WHITE), Some(1));
    }

    // 7. Verify line 492: if GpuTextures has NO WHITE texture either, batch is skipped
    {
        // Replace GpuTextures resource with empty one
        app.insert_resource(GpuTextures::default());
        let mut list = app.world_mut().resource_mut::<FrameDrawList>();
        list.0
            .reset(UVec2::new(800, 600), Vec4::new(0.0, 0.0, 0.0, 1.0));
        list.0.rect(
            custom_id,
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec4::ONE,
        );
    }
    app.update();
}

#[test]
fn test_material_specialization_and_shaders() {
    use bevy::render::render_resource::{
        ColorTargetState, ColorWrites, FragmentState, RenderPipelineDescriptor,
    };
    use bevy::sprite_render::Material2d;

    // Verify SceneMaterial shaders & alpha mode
    let scene_vert = SceneMaterial::vertex_shader();
    let scene_frag = SceneMaterial::fragment_shader();
    let mat = SceneMaterial {
        texture: Handle::default(),
    };
    assert_eq!(mat.alpha_mode(), bevy::sprite_render::AlphaMode2d::Blend);

    let mut desc = RenderPipelineDescriptor {
        label: None,
        layout: vec![],
        immediate_size: 0,
        vertex: bevy::render::render_resource::VertexState {
            shader: match scene_vert {
                bevy::shader::ShaderRef::Path(_) => Handle::default(),
                _ => Handle::default(),
            },
            shader_defs: vec![],
            entry_point: None,
            buffers: vec![],
        },
        primitive: default(),
        depth_stencil: None,
        multisample: default(),
        fragment: Some(FragmentState {
            shader: match scene_frag {
                bevy::shader::ShaderRef::Path(_) => Handle::default(),
                _ => Handle::default(),
            },
            shader_defs: vec![],
            entry_point: None,
            targets: vec![Some(ColorTargetState {
                format: bevy::render::render_resource::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        zero_initialize_workgroup_memory: false,
    };

    let mut layouts = bevy::mesh::MeshVertexBufferLayouts::default();
    let mesh_layout = bevy::mesh::MeshVertexBufferLayout::new(
        vec![],
        bevy::mesh::VertexBufferLayout {
            array_stride: 0,
            step_mode: bevy::render::render_resource::VertexStepMode::Vertex,
            attributes: vec![],
        },
    );
    let layout = layouts.insert(mesh_layout);
    let key = bevy::sprite_render::Material2dKey {
        mesh_key: bevy::sprite_render::Mesh2dPipelineKey::NONE,
        bind_group_data: default(),
    };

    assert!(SceneMaterial::specialize(&mut desc, &layout, key.clone()).is_ok());
    assert_eq!(
        desc.fragment.as_ref().unwrap().targets[0]
            .as_ref()
            .unwrap()
            .blend,
        Some(gource::render::GL_ALPHA_BLEND)
    );

    // Verify BloomMaterial shaders & alpha mode
    let _bloom_vert = BloomMaterial::vertex_shader();
    let _bloom_frag = BloomMaterial::fragment_shader();
    let bloom_mat = BloomMaterial::default();
    assert_eq!(
        bloom_mat.alpha_mode(),
        bevy::sprite_render::AlphaMode2d::Blend
    );

    let key_bloom = bevy::sprite_render::Material2dKey {
        mesh_key: bevy::sprite_render::Mesh2dPipelineKey::NONE,
        bind_group_data: default(),
    };
    assert!(BloomMaterial::specialize(&mut desc, &layout, key_bloom).is_ok());
    assert_eq!(
        desc.fragment.as_ref().unwrap().targets[0]
            .as_ref()
            .unwrap()
            .blend,
        Some(gource::render::GL_ADDITIVE_BLEND)
    );

    // Also test with multiple targets including None to exercise iter_mut().flatten() fully
    desc.fragment.as_mut().unwrap().targets = vec![
        None,
        Some(ColorTargetState {
            format: bevy::render::render_resource::TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask: ColorWrites::ALL,
        }),
    ];
    assert!(SceneMaterial::specialize(&mut desc, &layout, key.clone()).is_ok());
    assert_eq!(
        desc.fragment.as_ref().unwrap().targets[1]
            .as_ref()
            .unwrap()
            .blend,
        Some(gource::render::GL_ALPHA_BLEND)
    );

    // And test with fragment = None
    let mut no_frag_desc = desc.clone();
    no_frag_desc.fragment = None;
    assert!(SceneMaterial::specialize(&mut no_frag_desc, &layout, key).is_ok());
}

#[test]
fn test_draw_list_plugin_build() {
    use bevy::asset::AssetPlugin;
    use gource::render::DrawListPlugin;

    // Test with multisample true
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(DrawListPlugin { multisample: true });

    // Test with multisample false
    let mut app2 = App::new();
    app2.add_plugins(MinimalPlugins);
    app2.add_plugins(AssetPlugin::default());
    app2.add_plugins(DrawListPlugin { multisample: false });
}
