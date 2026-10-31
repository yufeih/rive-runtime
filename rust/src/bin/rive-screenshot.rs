use anyhow::Result;
use std::{env, fs, io, time::Duration};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let output = args
        .next()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    rive::set_audio_enabled(false)?;
    let mut surface = rive::Surface::offscreen()?;
    let mut renderer = surface.load(&fs::read(input)?)?;
    for arg in args {
        let arg = arg
            .to_str()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let data = arg
            .strip_prefix("--data=")
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let (name, value) = data
            .split_once('=')
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let found = match value {
            "true" => renderer.set_bool(name, true)?,
            "false" => renderer.set_bool(name, false)?,
            _ => renderer.set_string(name, value)?,
        };
        if !found {
            return Err(io::Error::from(io::ErrorKind::NotFound).into());
        }
    }
    for _ in 0..60 {
        renderer.advance(Duration::from_secs_f64(1.0 / 60.0));
    }
    let pixels = surface.screenshot(&mut renderer)?;
    let [width, height] = renderer.dimensions().map(|value| value.ceil() as u32);
    let mut encoder = png::Encoder::new(fs::File::create(output)?, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    Ok(())
}
