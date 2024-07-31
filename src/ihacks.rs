use image::{Pixel, Rgb, RgbImage, Rgba, RgbaImage};

#[cfg(feature = "simd")]
use std::arch::x86_64::*;

#[cfg(feature = "simd")]
pub fn comp(rx: u32, ry: u32, dst: &mut [u8; 256], src: &RgbaImage) {
    let sw = src.width();

    #[inline(always)]
    unsafe fn mask_alpha(v: __m128i) -> __m128i {
        _mm_and_si128(v, _mm_set1_epi32(0xFF))
    }

    #[inline(always)]
    unsafe fn mask_alpha_256(v: __m256i) -> __m256i {
        _mm256_and_si256(v, _mm256_set1_epi32(0xFF))
    }

    unsafe {
        // let src = _mm256_loadu_si256(
        //     src.as_raw().as_ptr().add((64 * ry + rx) as usize) as *const __m256i
        // );
        // let dsti = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);

        // let safa = mask_alpha_256(src);
        // let isafa = mask_alpha_256(_mm256_sub_epi32(_mm256_set1_epi32(255), safa));

        // let safa = _mm512_cvtepu8_epi16(safa);
        // let isafa = _mm512_cvtepu8_epi16(isafa);

        // let src2 = _mm512_cvtepu8_epi16(src);
        // let dst2 = _mm512_cvtepu8_epi16(dsti);

        // let cmp = _mm512_avg_epu16(
        //     _mm512_abs_epi16(_mm512_mulhi_epi16(src2, safa)),
        //     _mm512_abs_epi16(_mm512_mulhi_epi16(src2, dst2)),
        // );

        // let end = _mm512_cvtepi16_epi8(cmp);

        // _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, end);

        for pg in (0..64).step_by(4) {
            let src = _mm_loadu_si128(
                src.as_raw().as_ptr().add((64 * ry + rx + pg) as usize) as *const __m128i
            );
            let dsti = _mm_loadu_si128(dst.as_ptr().add(pg as usize) as *const __m128i);

            let safa = mask_alpha(src);

            let isafa = _mm256_cvtepu8_epi16(_mm_sub_epi32(_mm_set1_epi32(255), safa));
            let safa = _mm256_cvtepu8_epi16(safa);

            let src2 = _mm256_cvtepu8_epi16(src);
            let dst2 = _mm256_cvtepu8_epi16(dsti);

            let cmp = _mm256_avg_epu16(
                _mm256_abs_epi16(_mm256_mulhi_epi16(src2, safa)),
                _mm256_abs_epi16(_mm256_mulhi_epi16(dst2, isafa)),
            );

            let end = _mm256_cvtepi16_epi8(cmp);

            _mm_storeu_si128(dst.as_mut_ptr().add(pg as usize) as *mut __m128i, end);
        }

        // let cmp = _mm256_avg_epu8(, b)

        // for pg in (0..rw * rh).step_by(4) {
        //     // pixel indice
        //     let sp = _mm_loadu_si128(
        //         src.as_raw().as_ptr().add((sw * ry + rx + pg) as usize) as *const __m128i
        //     );

        //     // Extract alpha values
        //     let spa = mask_alpha(sp);
        //     // Calculate inverse alpha values
        //     let spai = mask_alpha(_mm_sub_epi32(_mm_set1_epi32(255), spa));

        //     // Load destination pixels
        //     let dp = _mm_loadu_si128(dst.as_ptr().add(pg as usize) as *mut __m128i);

        //     let cmp = _mm_avg_epu8(_mm_mulhi_epu16(sp, spa), _mm_mulhi_epu16(dp, spai));

        //     // Store blended pixels back to destination
        //     println!("{:?}", dst);
        //     _mm_storeu_si128(
        //         dst.as_mut_ptr().add(pg as usize) as *mut __m128i,
        //         _mm_or_si128(cmp, _mm_set1_epi32(0xFF)),
        //     );
        // }
    }
}
