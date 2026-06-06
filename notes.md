## gpu types

wgpu:
```
- BindGroup
  - Device::create_bind_group
    - BindGroupDescriptor
      - BindGroupLayout
        - Device::create_bind_group_layout
          - BindGroupLayoutDescriptor
            - []
              - BindGroupLayoutEntry
                - binding: u32
                - ShaderStages
                - BindingType
                  | Buffer
                  | Texture
                  | StorageTexture
                  | ...
                - count: Option<NonZero<u32>>
      - []
        - BindGroupEntry
          - binding: u32
          - BindingResource
            | BufferBinding
            | []
              - BufferBinding
            | TextureView
            | ...
```

bevy:
```
BindGroup: Deref<Target = wgpu::BindGroup>
- RenderDevice::create_bind_group
  - BindGroupLayout: Deref<Target = wgpu::BindGroupLayout>
    | RenderDevice::create_bind_group_layout
      - []
        - wgpu::BindGroupLayoutEntry
    | PipelineCache::get_bind_group_layout
      - BindGroupLayoutDescriptor // no relation to wgpu::BindGroupLayoutDescriptor
        - BindGroupLayoutDescriptor::new
          - []
            - wgpu::BindGroupLayoutEntry
  - []
    - wgpu::BindGroupEntry

BindGroupEntries: Deref<Target = [wgpu::BindGroupEntry]>
- BindGroupEntries::with_indices
  - impl IntoIndexedBindingArray
    - ((u32, T), ..) where T: IntoBinding
      - T is basically anything that can be in a wgpu::BindingResource

BindGroupLayoutEntries: Deref<Target = [wgpu::BindGroupLayoutEntry]>
- BindGroupEntries::with_indices
  - impl IntoIndexedBindGroupLayoutEntryBuilderArray
    - ((u32, T), ..) where T: IntoBindGroupLayoutEntryBuilder
      - T is wgpu::BindingType, wgpu::BindGroupLayoutEntry, or BindGroupLayoutEntryBuilder

ComputePipeline: Deref<Target = wgpu::ComputePipeline>
| RenderDevice::create_compute_pipeline
  - RawComputePipelineDescriptor, import alias for wgpu::ComputePipelineDescriptor
| PipelineCache::get_compute_pipeline
  - CachedComputePipelineId
    - PipelineCache::queue_compute_pipeline
      - ComputePipelineDescriptor
        - Vec
          - BindGroupLayoutDescriptor
        - Handle<Shader>
          - load_embedded_asset!
```
