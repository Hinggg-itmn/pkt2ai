//! Minimal Twofish-128 and EAX implementation matching Packet Tracer.

const M32: u32 = u32::MAX;

const QT: [[[u8; 16]; 4]; 2] = [
    [
        [
            0x8, 0x1, 0x7, 0xd, 0x6, 0xf, 0x3, 0x2, 0x0, 0xb, 0x5, 0x9, 0xe, 0xc, 0xa, 0x4,
        ],
        [
            0xe, 0xc, 0xb, 0x8, 0x1, 0x2, 0x3, 0x5, 0xf, 0x4, 0xa, 0x6, 0x7, 0x0, 0x9, 0xd,
        ],
        [
            0xb, 0xa, 0x5, 0xe, 0x6, 0xd, 0x9, 0x0, 0xc, 0x8, 0xf, 0x3, 0x2, 0x4, 0x7, 0x1,
        ],
        [
            0xd, 0x7, 0xf, 0x4, 0x1, 0x2, 0x6, 0xe, 0x9, 0xb, 0x3, 0x0, 0x8, 0x5, 0xc, 0xa,
        ],
    ],
    [
        [
            0x2, 0x8, 0xb, 0xd, 0xf, 0x7, 0x6, 0xe, 0x3, 0x1, 0x9, 0x4, 0x0, 0xa, 0xc, 0x5,
        ],
        [
            0x1, 0xe, 0x2, 0xb, 0x4, 0xc, 0x3, 0x7, 0x6, 0xd, 0xa, 0x5, 0xf, 0x9, 0x0, 0x8,
        ],
        [
            0x4, 0xc, 0x7, 0x5, 0x1, 0x6, 0x9, 0xa, 0x0, 0xe, 0xd, 0x8, 0x2, 0xb, 0x3, 0xf,
        ],
        [
            0xb, 0x9, 0x5, 0x1, 0xc, 0x3, 0xd, 0xe, 0x6, 0x4, 0x7, 0xf, 0x2, 0x0, 0x8, 0xa,
        ],
    ],
];

const MDS: [[u16; 4]; 4] = [
    [0x01, 0xef, 0x5b, 0x5b],
    [0x5b, 0xef, 0xef, 0x01],
    [0xef, 0x5b, 0x01, 0xef],
    [0xef, 0x01, 0xef, 0x5b],
];

const RS: [[u16; 8]; 4] = [
    [0x01, 0xa4, 0x55, 0x87, 0x5a, 0x58, 0xdb, 0x9e],
    [0xa4, 0x56, 0x82, 0xf3, 0x1e, 0xc6, 0x68, 0xe5],
    [0x02, 0xa1, 0xfc, 0xc1, 0x47, 0xae, 0x3d, 0x19],
    [0xa4, 0x55, 0x87, 0x5a, 0x58, 0xdb, 0x9e, 0x03],
];

fn ror4(x: u8, n: u32) -> u8 {
    ((x >> n) | (x << (4 - n))) & 15
}

fn make_q(table: &[[u8; 16]; 4]) -> [u8; 256] {
    let mut out = [0; 256];
    for (x, slot) in out.iter_mut().enumerate() {
        let a0 = (x >> 4) as u8;
        let b0 = (x & 15) as u8;
        let a1 = a0 ^ b0;
        let b1 = (a0 ^ ror4(b0, 1) ^ ((a0 << 3) & 15)) & 15;
        let a2 = table[0][a1 as usize];
        let b2 = table[1][b1 as usize];
        let a3 = a2 ^ b2;
        let b3 = (a2 ^ ror4(b2, 1) ^ ((a2 << 3) & 15)) & 15;
        let a4 = table[2][a3 as usize];
        let b4 = table[3][b3 as usize];
        *slot = (b4 << 4) | a4;
    }
    out
}

fn gf(mut a: u16, mut b: u16, polynomial: u16) -> u8 {
    let mut result = 0u16;
    while b != 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        a <<= 1;
        if a & 0x100 != 0 {
            a ^= polynomial;
        }
        b >>= 1;
    }
    result as u8
}

fn y_bytes(x: [u8; 4], l0: [u8; 4], l1: [u8; 4], q0: &[u8; 256], q1: &[u8; 256]) -> [u8; 4] {
    [
        q1[(q0[(q0[x[0] as usize] ^ l1[0]) as usize] ^ l0[0]) as usize],
        q0[(q0[(q1[x[1] as usize] ^ l1[1]) as usize] ^ l0[1]) as usize],
        q1[(q1[(q0[x[2] as usize] ^ l1[2]) as usize] ^ l0[2]) as usize],
        q0[(q1[(q1[x[3] as usize] ^ l1[3]) as usize] ^ l0[3]) as usize],
    ]
}

fn mds_word(y: [u8; 4]) -> u32 {
    let mut word = 0u32;
    for (i, row) in MDS.iter().enumerate() {
        let mut value = 0u8;
        for j in 0..4 {
            value ^= gf(row[j], y[j] as u16, 0x169);
        }
        word |= u32::from(value) << (8 * i);
    }
    word
}

fn h(x: u32, l: [u32; 2], q0: &[u8; 256], q1: &[u8; 256]) -> u32 {
    mds_word(y_bytes(
        x.to_le_bytes(),
        l[0].to_le_bytes(),
        l[1].to_le_bytes(),
        q0,
        q1,
    ))
}

/// Twofish with the 128-bit key size used by Packet Tracer.
pub struct Twofish128 {
    keys: [u32; 40],
    sbox: [[u32; 256]; 4],
}

impl Twofish128 {
    pub fn new(key: [u8; 16]) -> Self {
        let q0 = make_q(&QT[0]);
        let q1 = make_q(&QT[1]);
        let words: [u32; 4] = std::array::from_fn(|i| {
            u32::from_le_bytes(key[i * 4..i * 4 + 4].try_into().expect("four bytes"))
        });
        let even = [words[0], words[2]];
        let odd = [words[1], words[3]];
        let mut s = [0u32; 2];
        for i in 0..2 {
            let mut bytes = [0u8; 4];
            for row in 0..4 {
                for col in 0..8 {
                    bytes[row] ^= gf(RS[row][col], key[i * 8 + col] as u16, 0x14d);
                }
            }
            s[i] = u32::from_le_bytes(bytes);
        }
        let s = [s[1], s[0]];
        let mut keys = [0u32; 40];
        const RHO: u32 = 0x0101_0101;
        for i in 0..20u32 {
            let a = h((2 * i).wrapping_mul(RHO), even, &q0, &q1);
            let b = h((2 * i + 1).wrapping_mul(RHO), odd, &q0, &q1).rotate_left(8);
            keys[(2 * i) as usize] = a.wrapping_add(b) & M32;
            keys[(2 * i + 1) as usize] = a.wrapping_add(b.wrapping_mul(2)).rotate_left(9);
        }
        let l0 = s[0].to_le_bytes();
        let l1 = s[1].to_le_bytes();
        let mut sbox = [[0u32; 256]; 4];
        for column in 0..4 {
            for (x, slot) in sbox[column].iter_mut().enumerate() {
                let mut input = [0u8; 4];
                input[column] = x as u8;
                let y = y_bytes(input, l0, l1, &q0, &q1);
                let mut only = [0u8; 4];
                only[column] = y[column];
                *slot = mds_word(only);
            }
        }
        Self { keys, sbox }
    }

    pub fn encrypt_block(&self, block: [u8; 16]) -> [u8; 16] {
        let mut r = [0u32; 4];
        for i in 0..4 {
            r[i] = u32::from_le_bytes(block[i * 4..i * 4 + 4].try_into().expect("four bytes"))
                ^ self.keys[i];
        }
        for round in 0..16 {
            let t0 = self.g(r[0]);
            let t1 = self.g(r[1].rotate_left(8));
            let f0 = t0.wrapping_add(t1).wrapping_add(self.keys[2 * round + 8]);
            let f1 = t0
                .wrapping_add(t1.wrapping_mul(2))
                .wrapping_add(self.keys[2 * round + 9]);
            r[2] = (r[2] ^ f0).rotate_right(1);
            r[3] = r[3].rotate_left(1) ^ f1;
            r = [r[2], r[3], r[0], r[1]];
        }
        let out = [
            r[2] ^ self.keys[4],
            r[3] ^ self.keys[5],
            r[0] ^ self.keys[6],
            r[1] ^ self.keys[7],
        ];
        let mut bytes = [0u8; 16];
        for (i, word) in out.iter().enumerate() {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    fn g(&self, x: u32) -> u32 {
        self.sbox[0][(x & 255) as usize]
            ^ self.sbox[1][((x >> 8) & 255) as usize]
            ^ self.sbox[2][((x >> 16) & 255) as usize]
            ^ self.sbox[3][(x >> 24) as usize]
    }
}

fn xor_block(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    std::array::from_fn(|i| a[i] ^ b[i])
}

fn dbl(block: [u8; 16]) -> [u8; 16] {
    let mut value = u128::from_be_bytes(block) << 1;
    if block[0] & 0x80 != 0 {
        value ^= 0x87;
    }
    value.to_be_bytes()
}

fn cmac(cipher: &Twofish128, data: &[u8]) -> [u8; 16] {
    let k1 = dbl(cipher.encrypt_block([0; 16]));
    let k2 = dbl(k1);
    let (head, last) = if !data.is_empty() && data.len().is_multiple_of(16) {
        let split = data.len() - 16;
        let tail: [u8; 16] = data[split..].try_into().expect("full block");
        (&data[..split], xor_block(tail, k1))
    } else {
        let split = data.len() - data.len() % 16;
        let mut padded = [0u8; 16];
        padded[..data.len() - split].copy_from_slice(&data[split..]);
        padded[data.len() - split] = 0x80;
        (&data[..split], xor_block(padded, k2))
    };
    let mut state = [0u8; 16];
    for chunk in head.as_chunks::<16>().0 {
        state = cipher.encrypt_block(xor_block(state, *chunk));
    }
    cipher.encrypt_block(xor_block(state, last))
}

fn omac(cipher: &Twofish128, domain: u8, data: &[u8]) -> [u8; 16] {
    let mut prefixed = Vec::with_capacity(16 + data.len());
    prefixed.extend_from_slice(&[0; 15]);
    prefixed.push(domain);
    prefixed.extend_from_slice(data);
    cmac(cipher, &prefixed)
}

fn ctr(cipher: &Twofish128, counter: [u8; 16], data: &[u8]) -> Vec<u8> {
    let mut count = u128::from_be_bytes(counter);
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let stream = cipher.encrypt_block(count.to_be_bytes());
        out.extend(chunk.iter().zip(stream).map(|(a, b)| a ^ b));
        count = count.wrapping_add(1);
    }
    out
}

pub fn eax_encrypt(key: [u8; 16], nonce: [u8; 16], plaintext: &[u8]) -> (Vec<u8>, [u8; 16]) {
    let cipher = Twofish128::new(key);
    let n = omac(&cipher, 0, &nonce);
    let header = omac(&cipher, 1, &[]);
    let ciphertext = ctr(&cipher, n, plaintext);
    let tag = xor_block(xor_block(n, header), omac(&cipher, 2, &ciphertext));
    (ciphertext, tag)
}

pub fn eax_decrypt(
    key: [u8; 16],
    nonce: [u8; 16],
    ciphertext: &[u8],
    tag: [u8; 16],
) -> Option<Vec<u8>> {
    let cipher = Twofish128::new(key);
    let n = omac(&cipher, 0, &nonce);
    let expected = xor_block(
        xor_block(n, omac(&cipher, 1, &[])),
        omac(&cipher, 2, ciphertext),
    );
    let different = expected
        .iter()
        .zip(tag)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b));
    (different == 0).then(|| ctr(&cipher, n, ciphertext))
}

/// Decrypt without authenticating, for the diagnostic `--no-verify` flag.
pub fn eax_decrypt_unchecked(key: [u8; 16], nonce: [u8; 16], ciphertext: &[u8]) -> Vec<u8> {
    let cipher = Twofish128::new(key);
    let counter = omac(&cipher, 0, &nonce);
    ctr(&cipher, counter, ciphertext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn twofish_vectors() {
        let zero = Twofish128::new([0; 16]);
        assert_eq!(
            zero.encrypt_block([0; 16]).as_slice(),
            hex("9f589f5cf6122c32b6bfec2f2ae8c35a")
        );
        let packet_tracer = Twofish128::new([0x89; 16]);
        assert_eq!(
            packet_tracer.encrypt_block([0; 16]).as_slice(),
            hex("0cf79f98d6de19e22213559a7bb0d40c")
        );
        assert_eq!(
            packet_tracer
                .encrypt_block(std::array::from_fn(|i| i as u8))
                .as_slice(),
            hex("2e927ae611b22db655fc747ab4ef309f")
        );
    }

    #[test]
    fn eax_vectors() {
        let vectors = [
            (0, "", "88f98d6e5ae14558bfd229c85f2e82e3"),
            (1, "eb", "b61da3d7ba3f64d559e807bc1c185ae5"),
            (
                15,
                "ebf7f10fc145bc6c8a7d23406fb842",
                "1ed8acde061654d45ceb3c5ab0cc38fa",
            ),
            (
                16,
                "ebf7f10fc145bc6c8a7d23406fb842ce",
                "d205e2c5e2701e8ccb6f7bf2f85c089d",
            ),
            (
                17,
                "ebf7f10fc145bc6c8a7d23406fb842ce6f",
                "ac5fc07fd3bb634a4b9b3e263f1e5614",
            ),
            (
                33,
                "ebf7f10fc145bc6c8a7d23406fb842ce6f25115c2663d04ab4ee4cbc10f93d7c84",
                "8825adc8311b753dcb6b646a0582642f",
            ),
        ];
        for (length, expected_ct, expected_tag) in vectors {
            let plain: Vec<u8> = (0..length).map(|i| i as u8).collect();
            let (ct, tag) = eax_encrypt([0x89; 16], [0x10; 16], &plain);
            assert_eq!(ct, hex(expected_ct));
            assert_eq!(tag.as_slice(), hex(expected_tag));
            assert_eq!(
                eax_decrypt([0x89; 16], [0x10; 16], &ct, tag).unwrap(),
                plain
            );
        }
    }
}
