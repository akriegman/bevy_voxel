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
- BindGroup: Deref<Target = wgpu::BindGroup>
  - RenderDevice::create_bind_group
    - BindGroupLayout: Deref<Target = wgpu::BindGroupLayout>
      - RenderDevice::create_bind_group_layout
        - []
          - wgpu::BindGroupLayoutEntry
    - []
      - wgpu::BindGroupEntry

BindGroupEntries: Deref<Target = [BindGroupEntry]>
- BindGroupEntries::with_indices
  - impl IntoIndexedBindingArray
    - ((u32, T), ..) where T: IntoBinding
      - T is basically anything that can be in a BindingResource

BindGroupLayoutEntries: Deref<Target = [BindGroupLayoutEntry]>
- BindGroupEntries::with_indices
  - impl IntoIndexedBindGroupLayoutEntryBuilderArray
    - ((u32, T), ..) where T: IntoBindGroupLayoutEntryBuilder
      - T is wgpu::BindingType, wgpu::BindGroupLayoutEntry, or BindGroupLayoutEntryBuilder
```
