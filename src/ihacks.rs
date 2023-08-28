use image::{Pixel, Rgb, RgbImage, Rgba, RgbaImage};

fn amult(p: &mut Rgba<u8>) {
    p[0] *= p[3] / 255;
    p[1] *= p[3] / 255;
    p[2] *= p[3] / 255;
    p[3] = 255;
}

fn multr(p: &mut Rgb<u8>, a: u8) {
    let a = 255 - a;
    p[0] *= a / 255;
    p[1] *= a / 255;
    p[2] *= a / 255;
}

fn pplus(d: &mut Rgb<u8>, p: Rgb<u8>) {
    d[0] += p[0];
    d[1] += p[1];
    d[2] += p[2];
}

pub fn comp(rx: u32, ry: u32, rw: u32, rh: u32, dest: &mut RgbImage, source: &RgbaImage) {
    for x in 0..rw {
        for y in 0..rh {
            let mut pixel = source.get_pixel(x + rx, y + ry).to_owned();
            let mut sdest = dest.get_pixel(x, y).to_owned();
            // pixel.blend(&Rgb::to_rgba(&dest.get_pixel(x, y)));
            multr(&mut sdest, pixel[3]);
            amult(&mut pixel);
            let mut conv = Rgba::to_rgb(&pixel);
            pplus(&mut conv, sdest);
            dest.put_pixel(x, y, conv);
        }
    }
}
