//! Render the same artwork used by the launcher, with badges for the tray.
use crate::jobs::TrayStatus;
use gtk4::{
    cairo, gdk, gdk_pixbuf, glib,
    prelude::{GdkCairoContextExt, PixbufLoaderExt},
};

const ARTWORK: &[u8] = include_bytes!("../../../data/org.cloudsave.Cloudsave.svg");

fn pixbuf(size: i32) -> Result<gdk_pixbuf::Pixbuf, glib::Error> {
    let loader = gdk_pixbuf::PixbufLoader::with_type("svg")?;
    loader.set_size(size, size);
    loader.write(ARTWORK)?;
    loader.close()?;
    loader
        .pixbuf()
        .ok_or_else(|| draw_error("App-Icon konnte nicht geladen werden"))
}

pub fn texture() -> Result<gdk::Texture, glib::Error> {
    Ok(gdk::Texture::for_pixbuf(&pixbuf(128)?))
}

fn draw_error(error: impl std::fmt::Display) -> glib::Error {
    glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &error.to_string())
}

fn paint_badge(cr: &cairo::Context, status: TrayStatus, size: i32) -> Result<(), cairo::Error> {
    let color = match status {
        TrayStatus::Ready => return Ok(()),
        TrayStatus::Busy => (0.11, 0.34, 0.68),
        TrayStatus::Paused => (0.26, 0.27, 0.31),
        TrayStatus::Error => (0.70, 0.13, 0.19),
        TrayStatus::Success => (0.10, 0.43, 0.27),
    };
    cr.scale(f64::from(size) / 24.0, f64::from(size) / 24.0);
    cr.arc(17.0, 17.0, 6.0, 0.0, std::f64::consts::TAU);
    cr.set_source_rgb(color.0, color.1, color.2);
    cr.fill_preserve()?;
    cr.set_source_rgb(0.13, 0.10, 0.08);
    cr.set_line_width(1.0);
    cr.stroke()?;
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.set_line_width(1.6);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    match status {
        TrayStatus::Busy => {
            cr.move_to(15.5, 19.2);
            cr.line_to(15.5, 14.4);
            cr.line_to(13.8, 16.1);
            cr.move_to(18.5, 14.8);
            cr.line_to(18.5, 19.6);
            cr.line_to(20.2, 17.9);
        }
        TrayStatus::Paused => {
            cr.move_to(15.4, 14.6);
            cr.line_to(15.4, 19.4);
            cr.move_to(18.6, 14.6);
            cr.line_to(18.6, 19.4);
        }
        TrayStatus::Error => {
            cr.move_to(17.0, 14.2);
            cr.line_to(17.0, 17.0);
            cr.new_sub_path();
            cr.arc(17.0, 19.5, 0.3, 0.0, std::f64::consts::TAU);
        }
        TrayStatus::Success => {
            cr.move_to(14.2, 17.1);
            cr.line_to(16.1, 19.0);
            cr.line_to(20.1, 15.0);
        }
        TrayStatus::Ready => unreachable!(),
    }
    cr.stroke()
}

pub fn argb(size: i32, status: TrayStatus) -> Result<Vec<u8>, glib::Error> {
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).map_err(draw_error)?;
    {
        let cr = cairo::Context::new(&surface).map_err(draw_error)?;
        let cloud = pixbuf(size)?;
        cr.set_source_pixbuf(&cloud, 0.0, 0.0);
        cr.paint().map_err(draw_error)?;
        paint_badge(&cr, status, size).map_err(draw_error)?;
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let pixels = surface.data().map_err(draw_error)?;
    let mut result = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size as usize {
        for x in 0..size as usize {
            let offset = y * stride + x * 4;
            let native = u32::from_ne_bytes(pixels[offset..offset + 4].try_into().unwrap());
            let [a, r, g, b] = native.to_be_bytes();
            let straight = |channel: u8| {
                if a == 0 {
                    0
                } else {
                    ((u32::from(channel) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                }
            };
            result.extend_from_slice(&[a, straight(r), straight(g), straight(b)]);
        }
    }
    Ok(result)
}
