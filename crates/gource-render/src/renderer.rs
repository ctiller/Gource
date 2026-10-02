//! Plain-`wgpu` [`DrawList`] renderer for offscreen capture and surface presentation.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use gource_draw::{DrawList, Material, Texture, TextureId, TextureStore};

use crate::{
    plan::{
        FrameDrawData, GL_ADDITIVE_BLEND, GL_ALPHA_BLEND, GpuVertex, TextureAction, TextureTracker,
        padded_bytes_per_row, unpad_rgba_rows, wgpu_address_mode, wgpu_filter_mode,
        wgpu_mipmap_filter,
    },
    shader::{BLOOM_WGSL, SCENE_WGSL},
};

/// Errors produced when initializing or reading back from [`WgpuRenderer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// No compatible `wgpu` adapter was found on the system.
    NoAdapter,
    /// Creating the logical device failed.
    Device(String),
    /// Mapping the offscreen readback buffer failed.
    MapFailed,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no compatible wgpu adapter found"),
            Self::Device(msg) => write!(f, "failed to create wgpu device: {msg}"),
            Self::MapFailed => f.write_str("failed to map offscreen readback buffer"),
        }
    }
}

impl std::error::Error for RenderError {}

struct GpuTextureEntry {
    bind_group: wgpu::BindGroup,
}

/// Offscreen colour attachment, optional MSAA attachment, and staging buffer for CPU readback.
pub struct OffscreenTarget {
    width: u32,
    height: u32,
    sample_count: u32,
    color_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    msaa_view: Option<wgpu::TextureView>,
    readback_buffer: wgpu::Buffer,
    padded_row_bytes: u32,
}

impl OffscreenTarget {
    /// Allocate offscreen textures and staging buffer for `(width, height)`.
    pub fn new(device: &wgpu::Device, width: u32, height: u32, sample_count: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let sample_count = if sample_count > 1 { 4 } else { 1 };

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gource-offscreen-color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let msaa_view = (sample_count > 1).then(|| {
            let msaa_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gource-offscreen-msaa"),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            msaa_tex.create_view(&wgpu::TextureViewDescriptor::default())
        });

        let padded_row_bytes = padded_bytes_per_row(width);
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gource-offscreen-readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            width,
            height,
            sample_count,
            color_texture,
            color_view,
            msaa_view,
            readback_buffer,
            padded_row_bytes,
        }
    }

    /// Reallocate if the dimensions or sample count changed.
    pub fn ensure_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        let sample_count = if sample_count > 1 { 4 } else { 1 };
        if self.width != width || self.height != height || self.sample_count != sample_count {
            *self = Self::new(device, width, height, sample_count);
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }
}

/// Plain-`wgpu` renderer for [`DrawList`].
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    scene_pipeline: wgpu::RenderPipeline,
    bloom_pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    textures: HashMap<TextureId, GpuTextureEntry>,
    tracker: TextureTracker,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    index_buffer: wgpu::Buffer,
    index_capacity: usize,
    sample_count: u32,
}

impl WgpuRenderer {
    /// Initialize a headless `wgpu` renderer targeting `Rgba8Unorm`.
    pub fn new_headless(multisample: bool) -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = futures_lite::future::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            },
        ))
        .or_else(|_| {
            futures_lite::future::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: true,
                compatible_surface: None,
            }))
        })
        .map_err(|_| RenderError::NoAdapter)?;

        let (device, queue) = futures_lite::future::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default()),
        )
        .map_err(|e| RenderError::Device(e.to_string()))?;

        Ok(Self::from_device(
            device,
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            multisample,
        ))
    }

    /// Create a renderer from an existing `wgpu::Device` and `wgpu::Queue`.
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        target_format: wgpu::TextureFormat,
        multisample: bool,
    ) -> Self {
        let sample_count = if multisample { 4 } else { 1 };

        let viewport_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gource-viewport-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gource-viewport-uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gource-viewport-bg"),
            layout: &viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gource-texture-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        };

        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gource-scene-shader"),
            source: wgpu::ShaderSource::Wgsl(SCENE_WGSL.into()),
        });
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gource-bloom-shader"),
            source: wgpu::ShaderSource::Wgsl(BLOOM_WGSL.into()),
        });

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gource-scene-pipeline-layout"),
                bind_group_layouts: &[
                    Some(&viewport_bind_group_layout),
                    Some(&texture_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let bloom_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gource-bloom-pipeline-layout"),
                bind_group_layouts: &[Some(&viewport_bind_group_layout)],
                immediate_size: 0,
            });

        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gource-scene-pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene_shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: std::slice::from_ref(&vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: &scene_shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(GL_ALPHA_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let bloom_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gource-bloom-pipeline"),
            layout: Some(&bloom_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &bloom_shader,
                entry_point: Some("vertex"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: std::slice::from_ref(&vertex_layout),
            },
            fragment: Some(wgpu::FragmentState {
                module: &bloom_shader,
                entry_point: Some("fragment"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(GL_ADDITIVE_BLEND),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let initial_verts = 1024;
        let initial_indices = 1536;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gource-vertices"),
            size: (initial_verts * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gource-indices"),
            size: (initial_indices * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device,
            queue,
            scene_pipeline,
            bloom_pipeline,
            viewport_buffer,
            viewport_bind_group,
            texture_bind_group_layout,
            textures: HashMap::new(),
            tracker: TextureTracker::new(),
            vertex_buffer,
            vertex_capacity: initial_verts,
            index_buffer,
            index_capacity: initial_indices,
            sample_count,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Synchronize GPU textures with `store`, uploading only changed textures.
    pub fn sync_textures(&mut self, store: &TextureStore) {
        let actions = self.tracker.plan_sync(store);
        for action in actions {
            match action {
                TextureAction::Upload { id, version } => {
                    if let Some(tex) = store.get(id) {
                        let entry = self.upload_single_texture(tex);
                        self.textures.insert(id, entry);
                        self.tracker.record_upload(id, version);
                    }
                }
                TextureAction::Delete { id } => {
                    self.textures.remove(&id);
                    self.tracker.record_delete(id);
                }
            }
        }
    }

    fn upload_single_texture(&self, texture: &Texture) -> GpuTextureEntry {
        let width = texture.width.max(1);
        let height = texture.height.max(1);
        let mip_level_count = (texture.levels.len() as u32).max(1);

        let gpu_tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&texture.name),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        for (level, data) in texture.levels.iter().enumerate() {
            let mip_w = (width >> level).max(1);
            let mip_h = (height >> level).max(1);
            let expected = (mip_w * mip_h * 4) as usize;
            if data.len() >= expected {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &gpu_tex,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data[..expected],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip_w * 4),
                        rows_per_image: Some(mip_h),
                    },
                    wgpu::Extent3d {
                        width: mip_w,
                        height: mip_h,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        let view = gpu_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let address_mode = wgpu_address_mode(texture.options.wrap);
        let filter = wgpu_filter_mode(texture.options.filter);
        let mipmap_filter = wgpu_mipmap_filter(texture.levels.len());

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&texture.name),
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            address_mode_w: address_mode,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter,
            ..Default::default()
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&texture.name),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        GpuTextureEntry { bind_group }
    }

    fn prepare_frame(&mut self, list: &DrawList, store: &TextureStore) -> FrameDrawData {
        self.sync_textures(store);

        let vp = [
            (list.viewport.x.max(1)) as f32,
            (list.viewport.y.max(1)) as f32,
            0.0f32,
            0.0f32,
        ];
        self.queue
            .write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&vp));

        let data = FrameDrawData::from_draw_list(list);
        if data.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = data.vertices.len().next_power_of_two();
            self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gource-vertices"),
                size: (self.vertex_capacity * std::mem::size_of::<GpuVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if data.indices.len() > self.index_capacity {
            self.index_capacity = data.indices.len().next_power_of_two();
            self.index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gource-indices"),
                size: (self.index_capacity * std::mem::size_of::<u32>()) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        if !data.vertices.is_empty() {
            self.queue
                .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&data.vertices));
        }
        if !data.indices.is_empty() {
            self.queue
                .write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&data.indices));
        }

        data
    }

    fn record_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        list: &DrawList,
        data: &FrameDrawData,
        color_view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
    ) {
        let c = list.clear_colour;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gource-render-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(c.x),
                        g: f64::from(c.y),
                        b: f64::from(c.z),
                        a: f64::from(c.w),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        if data.is_empty() {
            return;
        }

        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        for batch in &data.batches {
            match batch.material {
                Material::Alpha => {
                    let Some(tex_entry) = self
                        .textures
                        .get(&batch.texture)
                        .or_else(|| self.textures.get(&TextureId::WHITE))
                    else {
                        continue;
                    };
                    pass.set_pipeline(&self.scene_pipeline);
                    pass.set_bind_group(1, &tex_entry.bind_group, &[]);
                }
                Material::Bloom => {
                    pass.set_pipeline(&self.bloom_pipeline);
                }
            }
            pass.draw_indexed(
                batch.index_offset..batch.index_offset + batch.index_count,
                0,
                0..1,
            );
        }
    }

    /// Render `list` into a caller-supplied texture view (e.g. a window surface).
    pub fn render_to_view(
        &mut self,
        list: &DrawList,
        store: &TextureStore,
        color_view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
    ) {
        let data = self.prepare_frame(list, store);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gource-view-encoder"),
            });
        self.record_pass(&mut encoder, list, &data, color_view, resolve_target);
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render `list` into `target` and return the tightly packed RGBA8 pixel buffer.
    pub fn render_offscreen(
        &mut self,
        list: &DrawList,
        store: &TextureStore,
        target: &mut OffscreenTarget,
    ) -> Result<Vec<u8>, RenderError> {
        let width = list.viewport.x.max(1);
        let height = list.viewport.y.max(1);
        target.ensure_size(&self.device, width, height, self.sample_count);

        let data = self.prepare_frame(list, store);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gource-offscreen-encoder"),
            });

        match &target.msaa_view {
            Some(msaa_view) => {
                self.record_pass(
                    &mut encoder,
                    list,
                    &data,
                    msaa_view,
                    Some(&target.color_view),
                );
            }
            None => {
                self.record_pass(&mut encoder, list, &data, &target.color_view, None);
            }
        }

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &target.readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = target.readback_buffer.slice(..);
        let result: Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>> =
            Arc::new(Mutex::new(None));
        let result_clone = Arc::clone(&result);
        slice.map_async(wgpu::MapMode::Read, move |res| {
            *result_clone.lock().unwrap_or_else(|e| e.into_inner()) = Some(res);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let map_ok = result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some_and(|r| r.is_ok());
        if !map_ok {
            return Err(RenderError::MapFailed);
        }

        let rgba = {
            let mapped = slice.get_mapped_range();
            unpad_rgba_rows(&mapped, width, height, target.padded_row_bytes)
        };
        target.readback_buffer.unmap();

        Ok(rgba)
    }
}
