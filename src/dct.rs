// Discrete Cosine Transform.
//
// Copyright 2023 Robbert Haarman
//
// SPDX-License-Identifier: MIT

//! This module implements the [Discrete Cosine Transform] (DCT) and
//! Inverse Discrete Cosine Transform (IDCT) using matrix operations.
//!
//! [Discrete Cosine Transform]: http://inglorion.net/documents/essays/data_compression/dct/

use std::convert::TryInto;

/// Everything in this module operates on square matrices of size N by N.
const N : usize = 8;

/// Lookup table for the DCT. This is logically an 8x8 matrix, here
/// represented as an array of 64 elements. The values shown here are
/// as computed by compute_dctlut in the test dctlut_matches_computed.
/// Some numbers have zeros added to the end to make the numbers line
/// up in the source code.
static DCTLUT : [f32; N * N] = [
    1.00000000,  1.00000000,  1.00000000,  1.00000000,  1.00000000,  1.00000000,  1.00000000,  1.00000000,
    0.98078525,  0.83146960,  0.55557020,  0.19509023, -0.19509032, -0.55557036, -0.83146966, -0.98078530,
    0.92387950,  0.38268343, -0.38268352, -0.92387960, -0.92387950, -0.38268313,  0.38268360,  0.92387956,
    0.83146960, -0.19509032, -0.98078530, -0.55557000,  0.55557007,  0.98078525,  0.19509053, -0.83146980,
    0.70710677, -0.70710677, -0.70710665,  0.70710700,  0.70710677, -0.70710725, -0.70710653,  0.70710680,
    0.55557020, -0.98078530,  0.19509041,  0.83146936, -0.83146980, -0.19509022,  0.98078513, -0.55557084,
    0.38268343, -0.92387950,  0.92387956, -0.38268390, -0.38268384,  0.92387930, -0.92387940,  0.38268390,
    0.19509023, -0.55557000,  0.83146936, -0.98078520, 0.980785400, -0.83147013,  0.55557114, -0.19509155
];

/// Lookup table for the Inverse DCT. This is logically an 8x8 matrix,
/// here represented as an array of 64 elements. This matrix has been
/// created by transposing DCTLUT and replacing the values in the first
/// column by 0.5
static IDCTLUT : [f32; N * N] = [
    0.5,  0.98078525,  0.92387950,  0.83146960,  0.70710677,  0.55557020,  0.38268343,  0.19509023,
    0.5,  0.83146960,  0.38268343, -0.19509032, -0.70710677, -0.98078530, -0.92387950, -0.55557000,
    0.5,  0.55557020, -0.38268352, -0.98078530, -0.70710665,  0.19509041,  0.92387956,  0.83146936,
    0.5,  0.19509023, -0.92387960, -0.55557000,  0.70710700,  0.83146936, -0.38268390, -0.98078520,
    0.5, -0.19509032, -0.92387950,  0.55557007,  0.70710677, -0.83146980, -0.38268384,  0.98078540,
    0.5, -0.55557036, -0.38268313,  0.98078525, -0.70710725, -0.19509022,  0.92387930, -0.83147013,
    0.5, -0.83146966,  0.38268360,  0.19509053, -0.70710653,  0.98078513, -0.92387940,  0.55557114,
    0.5, -0.98078530,  0.92387956, -0.83146980,  0.7071068,  -0.55557084,  0.38268390, -0.19509155,
];

/// Applies the forward DCT transform to an NxN matrix of image data.
pub fn transform(image: &[f32; N * N]) -> [f32; N * N] {
    matscale(
        &matmul_transposed(&matmul(&DCTLUT, image), &DCTLUT),
        0.015625)
}

/// Applies the inverse DCT transform to an NxN matrix of image data.
pub fn reverse(transformed: &[f32; N * N]) -> [f32; N * N] {
    matscale(
        &matmul_transposed(&matmul(&IDCTLUT, transformed), &IDCTLUT),
        4.0)
}

/// Multiplies two matrices.
fn matmul(a: &[f32; N * N], b: &[f32; N * N]) -> [f32; N * N] {
    let mut res : [f32; N * N] = vec![0.0; N * N].try_into().unwrap();
    for y in 0..N {
        for x in 0..N {
            res[y * N + x] =
                (0..N).map(|i| a[y * N + i] * b[i * N + x]).sum();
        }
    }
    res
}

/// Multiplies a matrix by the transpose of another matrix.
///
/// This is equivalent to the hypothetical `matmul(a, mattranspose(b))`.
fn matmul_transposed(a: &[f32; N * N], b: &[f32; N * N]) -> [f32; N * N] {
    let mut res : [f32; N * N] = vec![0.0; N * N].try_into().unwrap();
    for y in 0..N {
        for x in 0..N {
            res[y * N + x] =
                (0..N).map(|i| a[y * N + i] * b[x * N + i]).sum();
        }
    }
    res
}

/// Scales a matrix by a scalar factor.
///
/// This multiplies every element of the matrix by the scale factor.
fn matscale(a: &[f32; N * N], scale: f32) -> [f32; N * N] {
    let mut res : [f32; N * N] = vec![0.0; N * N].try_into().unwrap();
    for y in 0..N {
        for x in 0..N {
            res[y * N + x] = a[y * N + x] * scale;
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::convert::TryInto;

    /// Rounds the elements of a matrix to 3 decimal digits.
    fn round(a: &[f32; N * N]) -> [f32; N * N] {
        a.into_iter().map(|x| (*x * 1000.0).round() / 1000.0)
            .collect::<Vec<f32>>().try_into().unwrap()
    }

    #[test]
    fn dctlut_matches_computed() {
        fn compute_dctlut() -> Vec<f32> {
            (0..64).map(|i| {
                let x = (i % N) as f32;
                let y = (i / N) as f32;
                (std::f32::consts::PI * y / (N as f32) * (x + 0.5)).cos()
            }).collect()
        };
        assert_eq!(Vec::from(DCTLUT), compute_dctlut());
    }

    #[test]
    fn transform() {
        let img : [f32; N * N] = [
            0.444,  0.412,  0.372,  0.326,  0.252,  0.074, -0.616, -0.890,
            0.350,  0.380,  0.310,  0.350,  0.192, -0.168, -0.788, -0.882,
            0.412,  0.334,  0.380,  0.248,  0.130, -0.490, -0.874, -0.858,
            0.342,  0.342,  0.278,  0.232, -0.278, -0.742, -0.844, -0.882,
            0.318,  0.318,  0.270,  0.106, -0.546, -0.836, -0.828, -0.858,
            0.326,  0.278,  0.200, -0.294, -0.788, -0.858, -0.850, -0.858,
            0.270,  0.224, -0.066, -0.608, -0.882, -0.850, -0.844, -0.858,
            0.208,  0.074, -0.342, -0.874, -0.874, -0.890, -0.828, -0.804,
        ];
        let result = super::transform(&img);
        assert_eq!(round(&result), [
            -0.234,  0.322, -0.018, -0.017, -0.001, -0.004,  0.005, -0.002,
             0.136,  0.004, -0.079,  0.017,  0.017, -0.005,  0.003, -0.005,
            -0.007, -0.022,  0.009,  0.031, -0.012, -0.013,  0.006,  0.001,
             0.014,  0.004,  0.001, -0.004, -0.012,  0.009,  0.004, -0.007,
            -0.001, -0.005,  0.002,  0.001,  0.001,  0.004, -0.005, -0.002,
             0.002,  0.003,  0.002,  0.001, -0.003,  0.001,  0.000,  0.001,
             0.002,  0.001,  0.002,  0.001,  0.001, -0.001,  0.003,  0.006,
             0.005,  0.001, -0.002,  0.000,  0.001, -0.001,  0.001,  0.002,
        ]);
    }

    #[test]
    fn reversible() {
        let img : [f32; N * N] = [
            0.444,  0.412,  0.372,  0.326,  0.252,  0.074, -0.616, -0.890,
            0.350,  0.380,  0.310,  0.350,  0.192, -0.168, -0.788, -0.882,
            0.412,  0.334,  0.380,  0.248,  0.130, -0.490, -0.874, -0.858,
            0.342,  0.342,  0.278,  0.232, -0.278, -0.742, -0.844, -0.882,
            0.318,  0.318,  0.270,  0.106, -0.546, -0.836, -0.828, -0.858,
            0.326,  0.278,  0.200, -0.294, -0.788, -0.858, -0.850, -0.858,
            0.270,  0.224, -0.066, -0.608, -0.882, -0.850, -0.844, -0.858,
            0.208,  0.074, -0.342, -0.874, -0.874, -0.890, -0.828, -0.804,
        ];
        let transformed = super::transform(&img);
        let reversed = reverse(&transformed);
        assert_eq!(round(&reversed), img);
    }
}
