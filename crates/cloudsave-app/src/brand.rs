//! The orange cloud in the header and the tray share this renderer. No host icon
//! theme, SVG loader or installed Flatpak icon lookup is required by the panel.
use gtk4::{cairo, glib};

pub fn paint(cr: &cairo::Context, width: f64, height: f64) {
    let _ = cr.save();
    cr.scale(width / 128.0, height / 128.0);
    let gradient = cairo::LinearGradient::new(4.0, 4.0, 100.0, 124.0);
    gradient.add_color_stop_rgb(0.0, 1.0, 0.733, 0.271);
    gradient.add_color_stop_rgb(0.5, 1.0, 0.549, 0.059);
    gradient.add_color_stop_rgb(1.0, 0.839, 0.353, 0.0);
    crate::rounded_rect(cr, 4.0, 4.0, 120.0, 120.0, 29.0);
    let _ = cr.set_source(&gradient);
    let _ = cr.fill();
    crate::rounded_rect(cr, 5.0, 5.0, 118.0, 118.0, 28.0);
    cr.set_source_rgba(1.0, 0.824, 0.549, 0.65);
    cr.set_line_width(2.0);
    let _ = cr.stroke();
    cr.move_to(43.0, 87.0);
    cr.curve_to(17.0, 87.0, 16.0, 55.0, 40.0, 53.0);
    cr.curve_to(42.0, 22.0, 84.0, 22.0, 88.0, 53.0);
    cr.curve_to(114.0, 51.0, 116.0, 87.0, 88.0, 87.0);
    cr.close_path();
    cr.set_source_rgb(1.0, 0.98, 0.949);
    cr.set_line_width(7.0);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let _ = cr.stroke();
    let _ = cr.restore();
}

pub fn argb(size: i32) -> Result<Vec<u8>, glib::Error> {
    let error = |message: String| glib::Error::new(gtk4::gio::IOErrorEnum::Failed, &message);
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size)
        .map_err(|e| error(e.to_string()))?;
    {
        let cr = cairo::Context::new(&surface).map_err(|e| error(e.to_string()))?;
        paint(&cr, size as f64, size as f64);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let pixels = surface.data().map_err(|e| error(e.to_string()))?;
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
