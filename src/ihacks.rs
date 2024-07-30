use image::{Pixel, Rgb, RgbImage, Rgba, RgbaImage};

#[cfg(feature = "simd")]
use std::arch::x86_64::*;

#[cfg(not(feature = "simd"))] 
#[inline(always)]
fn amult(p: &mut Rgba<u8>) {
    p[0] *= p[3] / 255;
    p[1] *= p[3] / 255;
    p[2] *= p[3] / 255;
    p[3] = 255;
}

#[cfg(not(feature = "simd"))] 
#[inline(always)]
fn multr(p: &mut Rgb<u8>, a: u8) {
    let a = 255 - a;
    p[0] *= a / 255;
    p[1] *= a / 255;
    p[2] *= a / 255;
}

#[cfg(not(feature = "simd"))] 
#[inline(always)]
fn pplus(d: &mut Rgb<u8>, p: Rgb<u8>) {
    d[0] += p[0];
    d[1] += p[1];
    d[2] += p[2];
}

#[cfg(not(feature = "simd"))] 
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

#[cfg(feature = "simd")]
pub fn comp(rx: u32, ry: u32, rw: u32, rh: u32, dst: &mut RgbImage, src: &RgbaImage) {
    // Ensure the destination image is aligned to 16 bytes
    assert!(dst.as_raw().as_ptr() as usize % 16 == 0);

    unsafe {
        for y in (0..rh).step_by(2) {
            for x in (0..rw).step_by(2) {
                // Load source pixels
                let p0 = _mm_loadu_si128(
                    src.as_raw()
                        .as_ptr()
                        .add((x + rx) as usize * 4 + (y + ry) as usize * src.width() as usize * 4)
                        as *const __m128i,
                );
                let p1 =
                    _mm_loadu_si128(src.as_raw().as_ptr().add(
                        (x + rx + 1) as usize * 4 + (y + ry) as usize * src.width() as usize * 4,
                    ) as *const __m128i);
                let p2 =
                    _mm_loadu_si128(src.as_raw().as_ptr().add(
                        (x + rx) as usize * 4 + (y + ry + 1) as usize * src.width() as usize * 4,
                    ) as *const __m128i);
                let p3 = _mm_loadu_si128(src.as_raw().as_ptr().add(
                    (x + rx + 1) as usize * 4 + (y + ry + 1) as usize * src.width() as usize * 4,
                ) as *const __m128i);

                // Extract alpha values
                let alpha0 = _mm_and_si128(p0, _mm_set1_epi16(0xFF));
                let alpha1 = _mm_and_si128(p1, _mm_set1_epi16(0xFF));
                let alpha2 = _mm_and_si128(p2, _mm_set1_epi16(0xFF));
                let alpha3 = _mm_and_si128(p3, _mm_set1_epi16(0xFF));

                // Calculate inverse alpha values
                let inv_alpha0 = _mm_sub_epi8(_mm_set1_epi16(255), alpha0);
                let inv_alpha1 = _mm_sub_epi8(_mm_set1_epi16(255), alpha1);
                let inv_alpha2 = _mm_sub_epi8(_mm_set1_epi16(255), alpha2);
                let inv_alpha3 = _mm_sub_epi8(_mm_set1_epi16(255), alpha3);

                // Load destination pixels
                let d0 = _mm_loadu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add(x as usize * 3 + y as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                );
                let d1 = _mm_loadu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add((x + 1) as usize * 3 + y as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                );
                let d2 = _mm_loadu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add(x as usize * 3 + (y + 1) as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                );
                let d3 = _mm_loadu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add((x + 1) as usize * 3 + (y + 1) as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                );

                // Blend source and destination pixels
                let r0 = _mm_avg_epu8(_mm_mulhi_epu16(p0, alpha0), _mm_mulhi_epu16(d0, inv_alpha0));
                let r1 = _mm_avg_epu8(_mm_mulhi_epu16(p1, alpha1), _mm_mulhi_epu16(d1, inv_alpha1));
                let r2 = _mm_avg_epu8(_mm_mulhi_epu16(p2, alpha2), _mm_mulhi_epu16(d2, inv_alpha2));
                let r3 = _mm_avg_epu8(_mm_mulhi_epu16(p3, alpha3), _mm_mulhi_epu16(d3, inv_alpha3));

                // Store blended pixels back to destination
                _mm_storeu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add(x as usize * 3 + y as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                    r0,
                );
                _mm_storeu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add((x + 1) as usize * 3 + y as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                    r1,
                );
                _mm_storeu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add(x as usize * 3 + (y + 1) as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                    r2,
                );
                _mm_storeu_si128(
                    dst.as_raw()
                        .as_ptr()
                        .add((x + 1) as usize * 3 + (y + 1) as usize * dst.width() as usize * 3)
                        as *mut __m128i,
                    r3,
                );
            }
        }
    }
}
