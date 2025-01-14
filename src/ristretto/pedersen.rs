// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{borrow::Borrow, iter::Sum};

use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::RistrettoPoint,
    scalar::Scalar,
    traits::MultiscalarMul,
};

use crate::{
    commitment::{HomomorphicCommitment, HomomorphicCommitmentFactory},
    ristretto::{constants::RISTRETTO_NUMS_POINTS, RistrettoPublicKey, RistrettoSecretKey},
};

pub const RISTRETTO_PEDERSEN_G: RistrettoPoint = RISTRETTO_BASEPOINT_POINT;
lazy_static! {
    pub static ref RISTRETTO_PEDERSEN_H: RistrettoPoint = RISTRETTO_NUMS_POINTS[0];
}

pub type PedersenCommitment = HomomorphicCommitment<RistrettoPublicKey>;

/// Generates Pederson commitments `k.G + v.H` using the provided base
/// [RistrettoPoints](curve25519_dalek::ristretto::RistrettoPoints).
#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_snake_case)]
pub struct PedersenCommitmentFactory {
    pub(crate) G: RistrettoPoint,
    pub(crate) H: RistrettoPoint,
}

impl PedersenCommitmentFactory {
    /// Create a new Ristretto Commitment factory with the given points as the bases. It's very cheap to create
    /// factories, since we only hold references to the static generator points.
    #[allow(non_snake_case)]
    pub fn new(G: RistrettoPoint, H: RistrettoPoint) -> PedersenCommitmentFactory {
        PedersenCommitmentFactory { G, H }
    }
}

impl Default for PedersenCommitmentFactory {
    /// The default Ristretto Commitment factory uses the Base point for x25519 and its first Blake256 hash.
    fn default() -> Self {
        PedersenCommitmentFactory::new(RISTRETTO_PEDERSEN_G, *RISTRETTO_PEDERSEN_H)
    }
}

impl HomomorphicCommitmentFactory for PedersenCommitmentFactory {
    type P = RistrettoPublicKey;

    fn commit(&self, k: &RistrettoSecretKey, v: &RistrettoSecretKey) -> PedersenCommitment {
        let c = RistrettoPoint::multiscalar_mul(&[v.0, k.0], &[self.H, self.G]);
        HomomorphicCommitment(RistrettoPublicKey::new_from_pk(c))
    }

    fn zero(&self) -> PedersenCommitment {
        let zero = Scalar::zero();
        let c = RistrettoPoint::multiscalar_mul(&[zero, zero], &[self.H, self.G]);
        HomomorphicCommitment(RistrettoPublicKey::new_from_pk(c))
    }

    fn open(&self, k: &RistrettoSecretKey, v: &RistrettoSecretKey, commitment: &PedersenCommitment) -> bool {
        let c_test = self.commit(k, v);
        commitment.0 == c_test.0
    }

    fn commit_value(&self, k: &RistrettoSecretKey, value: u64) -> PedersenCommitment {
        let v = RistrettoSecretKey::from(value);
        self.commit(k, &v)
    }

    fn open_value(&self, k: &RistrettoSecretKey, v: u64, commitment: &HomomorphicCommitment<Self::P>) -> bool {
        let kv = RistrettoSecretKey::from(v);
        self.open(k, &kv, commitment)
    }
}

impl<T> Sum<T> for PedersenCommitment
where T: Borrow<PedersenCommitment>
{
    fn sum<I>(iter: I) -> Self
    where I: Iterator<Item = T> {
        let mut total = RistrettoPoint::default();
        for c in iter {
            let commitment = c.borrow();
            total += RistrettoPoint::from(&commitment.0);
        }
        let sum = RistrettoPublicKey::new_from_pk(total);
        HomomorphicCommitment(sum)
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::hash_map::DefaultHasher,
        convert::From,
        hash::{Hash, Hasher},
    };

    use tari_utilities::{message_format::MessageFormat, ByteArray};

    use super::*;
    use crate::keys::{PublicKey, SecretKey};

    #[test]
    fn check_default_base() {
        let base = PedersenCommitmentFactory::default();
        assert_eq!(base.G, RISTRETTO_PEDERSEN_G);
        assert_eq!(base.H, *RISTRETTO_PEDERSEN_H)
    }

    #[test]
    fn pubkey_roundtrip() {
        let mut rng = rand::thread_rng();
        let (_, p) = RistrettoPublicKey::random_keypair(&mut rng);
        let c = PedersenCommitment::from_public_key(&p);
        assert_eq!(c.as_public_key(), &p);
        let c2 = PedersenCommitment::from_bytes(c.as_bytes()).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn commitment_sub() {
        let mut rng = rand::thread_rng();
        let (_, a) = RistrettoPublicKey::random_keypair(&mut rng);
        let (_, b) = RistrettoPublicKey::random_keypair(&mut rng);
        let c = &a + &b;
        let a = PedersenCommitment::from_public_key(&a);
        let b = PedersenCommitment::from_public_key(&b);
        let c = PedersenCommitment::from_public_key(&c);
        assert_eq!(b, &c - &a);
    }

    #[test]
    fn check_g_ne_h() {
        assert_ne!(RISTRETTO_PEDERSEN_G, *RISTRETTO_PEDERSEN_H);
    }

    /// Simple test for open: Generate 100 random sets of scalars and calculate the Pedersen commitment for them.
    /// Then check that the commitment = k.G + v.H, and that `open` returns `true` for `open(&k, &v)`
    #[test]
    #[allow(non_snake_case)]
    fn check_open() {
        let factory = PedersenCommitmentFactory::default();
        let H = *RISTRETTO_PEDERSEN_H;
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let v = RistrettoSecretKey::random(&mut rng);
            let k = RistrettoSecretKey::random(&mut rng);
            let c = factory.commit(&k, &v);
            let c_calc: RistrettoPoint = v.0 * H + k.0 * RISTRETTO_PEDERSEN_G;
            assert_eq!(RistrettoPoint::from(c.as_public_key()), c_calc);
            assert!(factory.open(&k, &v, &c));
            // A different value doesn't open the commitment
            assert!(!factory.open(&k, &(&v + &v), &c));
            // A different blinding factor doesn't open the commitment
            assert!(!factory.open(&(&k + &v), &v, &c));
        }
    }

    /// Test, for 100 random sets of scalars that the homomorphic property holds. i.e.
    /// $$
    ///   C = C_1 + C_2 = (k_1+k_2).G + (v_1+v_2).H
    /// $$
    /// and
    /// `open(k1+k2, v1+v2)` is true for _C_
    #[test]
    fn check_homomorphism() {
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let v1 = RistrettoSecretKey::random(&mut rng);
            let v2 = RistrettoSecretKey::random(&mut rng);
            let v_sum = &v1 + &v2;
            let k1 = RistrettoSecretKey::random(&mut rng);
            let k2 = RistrettoSecretKey::random(&mut rng);
            let k_sum = &k1 + &k2;
            let factory = PedersenCommitmentFactory::default();
            let c1 = factory.commit(&k1, &v1);
            let c2 = factory.commit(&k2, &v2);
            let c_sum = &c1 + &c2;
            let c_sum2 = factory.commit(&k_sum, &v_sum);
            assert!(factory.open(&k1, &v1, &c1));
            assert!(factory.open(&k2, &v2, &c2));
            assert_eq!(c_sum, c_sum2);
            assert!(factory.open(&k_sum, &v_sum, &c_sum));
        }
    }

    /// Test addition of a public key to a homomorphic commitment.
    /// $$
    ///   C = C_1 + P = (v_1.H + k_1.G) + k_2.G = v_1.H + (k_1 + k_2).G
    /// $$
    /// and
    /// `open(k1+k2, v1)` is true for _C_
    #[test]
    fn check_homomorphism_with_public_key() {
        let mut rng = rand::thread_rng();
        // Left-hand side
        let v1 = RistrettoSecretKey::random(&mut rng);
        let k1 = RistrettoSecretKey::random(&mut rng);
        let factory = PedersenCommitmentFactory::default();
        let c1 = factory.commit(&k1, &v1);
        let (k2, k2_pub) = RistrettoPublicKey::random_keypair(&mut rng);
        let c_sum = &c1 + &k2_pub;
        // Right-hand side
        let c2 = factory.commit(&(&k1 + &k2), &v1);
        // Test
        assert_eq!(c_sum, c2);
        assert!(factory.open(&(&k1 + &k2), &v1, &c2));
    }

    #[test]
    fn sum_commitment_vector() {
        let mut rng = rand::thread_rng();
        let mut v_sum = RistrettoSecretKey::default();
        let mut k_sum = RistrettoSecretKey::default();
        let zero = RistrettoSecretKey::default();
        let commitment_factory = PedersenCommitmentFactory::default();
        let mut c_sum = commitment_factory.commit(&zero, &zero);
        let mut commitments = Vec::with_capacity(100);
        for _ in 0..100 {
            let v = RistrettoSecretKey::random(&mut rng);
            v_sum = &v_sum + &v;
            let k = RistrettoSecretKey::random(&mut rng);
            k_sum = &k_sum + &k;
            let c = commitment_factory.commit(&k, &v);
            c_sum = &c_sum + &c;
            commitments.push(c);
        }
        assert!(commitment_factory.open(&k_sum, &v_sum, &c_sum));
        assert_eq!(c_sum, commitments.iter().sum());
    }

    #[test]
    fn serialize_deserialize() {
        let mut rng = rand::thread_rng();
        let factory = PedersenCommitmentFactory::default();
        let k = RistrettoSecretKey::random(&mut rng);
        let c = factory.commit_value(&k, 420);
        // Base64
        let ser_c = c.to_base64().unwrap();
        let c2 = PedersenCommitment::from_base64(&ser_c).unwrap();
        assert!(factory.open_value(&k, 420, &c2));
        // MessagePack
        let ser_c = c.to_binary().unwrap();
        let c2 = PedersenCommitment::from_binary(&ser_c).unwrap();
        assert!(factory.open_value(&k, 420, &c2));
        // Invalid Base64
        assert!(PedersenCommitment::from_base64("bad@ser$").is_err());
    }

    #[test]
    fn derived_methods() {
        let factory = PedersenCommitmentFactory::default();
        let k = RistrettoSecretKey::from(1024);
        let c1 = factory.commit_value(&k, 2048);
        // Test Debug impl
        assert_eq!(
            format!("{:?}", c1),
            "HomomorphicCommitment(f09a7f46c5e3cbadc4c1e84c10278cffab2cb902f7b6f37223c88dd548877a6a)"
        );
        // test Clone impl
        let c2 = c1.clone();
        assert_eq!(c1, c2);
        // test hash impl
        let mut hasher = DefaultHasher::new();
        c1.hash(&mut hasher);
        let result = format!("{:x}", hasher.finish());
        assert_eq!(&result, "b1b43e91f6d6109f");
        // test Ord and PartialOrd impl
        let c3 = factory.commit_value(&k, 2049);
        assert!(c2 > c3);
        assert!(c2 != c3);
        assert!(c3 < c2);
        assert!(matches!(c2.cmp(&c3), std::cmp::Ordering::Greater));
    }

    #[test]
    fn default_value() {
        let c = PedersenCommitment::default();
        assert_eq!(c, PedersenCommitment::from_public_key(&RistrettoPublicKey::default()));
    }
}
