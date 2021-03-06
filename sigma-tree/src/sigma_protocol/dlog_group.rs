//! This is the general interface for the discrete logarithm prime-order group.
//!
//! The discrete logarithm problem is as follows: given a generator g of a finite
//! group G and a random element h in G, find the (unique) integer x such that
//! `g^x = h`.
//!
//! In cryptography, we are interested in groups for which the discrete logarithm problem
//! (Dlog for short) is assumed to be hard. The most known groups of that kind are some Elliptic curve groups.
//!
//! Another issue pertaining elliptic curves is the need to find a suitable mapping that will convert an arbitrary
//! message (that is some binary string) to an element of the group and vice-versa.
//!
//! Only a subset of the messages can be effectively mapped to a group element in such a way that there is a one-to-one
//! injection that converts the string to a group element and vice-versa.
//!
//! On the other hand, any group element can be mapped to some string.

use super::DlogProverInput;
use k256::arithmetic::{ProjectivePoint, Scalar};
use k256::{arithmetic::AffinePoint, PublicKey};
use num_bigint::{BigInt, Sign};
use sigma_ser::{
    serializer::{SerializationError, SigmaSerializable},
    vlq_encode,
};
use std::{convert::TryInto, io};

#[derive(PartialEq, Debug, Clone)]
pub struct EcPoint(ProjectivePoint);

impl EcPoint {
    pub const GROUP_SIZE: usize = 33;
}

impl Eq for EcPoint {}

/// The generator g of the group is an element of the group such that, when written multiplicatively, every element
/// of the group is a power of g.
pub fn generator() -> EcPoint {
    EcPoint(ProjectivePoint::generator())
}

/// the identity(infinity) element of this Dlog group
pub const fn identity() -> EcPoint {
    EcPoint(ProjectivePoint::identity())
}

pub fn is_identity(ge: &EcPoint) -> bool {
    *ge == identity()
}

/// Raises the base GroupElement to the exponent. The result is another GroupElement.
pub fn exponentiate(base: &EcPoint, exponent: &Scalar) -> EcPoint {
    if !is_identity(base) {
        // implement for negative exponent
        // see reference impl https://github.com/ScorexFoundation/sigmastate-interpreter/blob/ec71a6f988f7412bc36199f46e7ad8db643478c7/sigmastate/src/main/scala/sigmastate/basics/BcDlogGroup.scala#L201
        // see https://github.com/ergoplatform/sigma-rust/issues/36

        // we treat EC as a multiplicative group, therefore, exponentiate point is multiply.
        EcPoint(base.0 * exponent)
    } else {
        base.clone()
    }
}

/// Creates a random member of this Dlog group
pub fn random_element() -> EcPoint {
    let sk = DlogProverInput::random();
    let bytes = sk.w.to_bytes();
    let bi = BigInt::from_bytes_be(Sign::Plus, &bytes[..]);

    exponentiate(&generator(), &sk.w)
}

/// Creates a random scalar, a big-endian integer in the range [0, n), where n is group order
pub fn random_scalar_in_group_range() -> Scalar {
    loop {
        // Generate a new secret key using the operating system's
        // cryptographically secure random number generator
        let sk = k256::SecretKey::generate();
        let bytes: [u8; 32] = sk
            .secret_scalar()
            .as_ref()
            .as_slice()
            .try_into()
            .expect("expected 32 bytes");
        // Returns None if the byte array does not contain
        // a big-endian integer in the range [0, n), where n is group order.
        let maybe_scalar = Scalar::from_bytes(bytes);
        if bool::from(maybe_scalar.is_some()) {
            break maybe_scalar.unwrap();
        }
    }
}

impl SigmaSerializable for EcPoint {
    fn sigma_serialize<W: vlq_encode::WriteSigmaVlqExt>(&self, w: &mut W) -> Result<(), io::Error> {
        let caff = self.0.to_affine();
        if bool::from(caff.is_some()) {
            let pubkey = caff.unwrap().to_compressed_pubkey();
            w.write_all(pubkey.as_bytes())?;
        } else {
            // infinity point
            let zeroes = [0u8; EcPoint::GROUP_SIZE];
            w.write_all(&zeroes)?;
        }
        Ok(())
    }

    fn sigma_parse<R: vlq_encode::ReadSigmaVlqExt>(r: &mut R) -> Result<Self, SerializationError> {
        let mut buf = [0; EcPoint::GROUP_SIZE];
        r.read_exact(&mut buf[..])?;
        if buf[0] != 0 {
            let pubkey = PublicKey::from_bytes(&buf[..]).ok_or_else(|| {
                SerializationError::Misc("failed to parse PK from bytes".to_string())
            })?;
            let cp = AffinePoint::from_pubkey(&pubkey);
            if bool::from(cp.is_none()) {
                Err(SerializationError::Misc(
                    "failed to get affine point from PK".to_string(),
                ))
            } else {
                Ok(EcPoint(ProjectivePoint::from(cp.unwrap())))
            }
        } else {
            // infinity point
            Ok(EcPoint(ProjectivePoint::identity()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use sigma_ser::test_helpers::*;

    impl Arbitrary for EcPoint {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            prop_oneof![Just(generator()), Just(identity()), Just(random_element()),].boxed()
        }
    }

    proptest! {

        #[test]
        fn ser_roundtrip(v in any::<EcPoint>()) {
            prop_assert_eq![sigma_serialize_roundtrip(&v), v];
        }
    }
}
