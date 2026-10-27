// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Embedded icons and texture creation helpers.

use wgpu::util::DeviceExt;

/// Embedded PNG icons (compile-time).
pub(crate) const ICON_WALL: &[u8] = include_bytes!("../icons/wall.png");
pub(crate) const ICON_VALVE: &[u8] = include_bytes!("../icons/valve.png");
pub(crate) const ICON_THERMO: &[u8] = include_bytes!("../icons/thermo.png");
pub(crate) const ICON_SOURCE_OPEN: &[u8] = include_bytes!("../icons/sourceOpen.png");
pub(crate) const ICON_SOURCE_CLOSED: &[u8] = include_bytes!("../icons/sourceClosed.png");

/// Decode an embedded PNG into RGBA8, resized to `size×size`.
pub(crate) fn decode_icon(png_bytes: &[u8], size: u32) -> Vec<u8> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .expect("Failed to decode embedded PNG icon");
    let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
    resized.to_rgba8().into_raw()
}

/// Stack `icons` vertically into one sRGB atlas, `cell×cell` pixels each.
/// Icon `i` occupies V range `[i / n, (i + 1) / n)`.
pub(crate) fn icon_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    icons: &[&[u8]],
    cell: u32,
) -> wgpu::TextureView {
    let data: Vec<u8> = icons
        .iter()
        .flat_map(|png| decode_icon(png, cell))
        .collect();
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: extent(cell, cell * icons.len() as u32),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &data,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Decode a PNG at full resolution and upload it with a full mip chain, so it
/// stays clean when drawn small (zoomed out).
pub(crate) fn mipmapped_icon(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    png_bytes: &[u8],
) -> wgpu::TextureView {
    let img = image::load_from_memory(png_bytes)
        .expect("Failed to decode embedded PNG icon")
        .to_rgba8();
    let (width, height) = img.dimensions();

    // floor(log2(max side)) + 1 levels; wgpu mip sizes are max(1, dim >> level).
    let mip_count = 32 - width.max(height).leading_zeros();
    let mut data: Vec<u8> = Vec::new();
    let mut level = img;
    for i in 0..mip_count {
        data.extend_from_slice(level.as_raw());
        if i + 1 < mip_count {
            let (w, h) = ((level.width() / 2).max(1), (level.height() / 2).max(1));
            level = image::imageops::resize(&level, w, h, image::imageops::FilterType::Triangle);
        }
    }

    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: extent(width, height),
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &data,
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Single-channel `R8Unorm` texture that can be rewritten with [`write_r8`].
pub(crate) fn r8_texture(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Overwrite an `R8Unorm` texture (row-major, `width` bytes per row).
pub(crate) fn write_r8(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    data: &[u8],
    width: u32,
    height: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        extent(width, height),
    );
}

fn extent(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}
