//! ErgoTree
use crate::{
    ast::{Constant, Expr},
    types::SType,
};
use io::{Cursor, Read};
use sigma_ser::serializer::SerializationError;
use sigma_ser::serializer::SigmaSerializable;
use sigma_ser::{peekable_reader::PeekableReader, vlq_encode};
use std::io;
use std::rc::Rc;
use vlq_encode::{ReadSigmaVlqExt, WriteSigmaVlqExt};

#[derive(PartialEq, Eq, Debug, Clone)]
struct ParsedTree {
    constants: Vec<Constant>,
    root: Result<Rc<Expr>, ErgoTreeRootParsingError>,
}

/** The root of ErgoScript IR. Serialized instances of this class are self sufficient and can be passed around.
 */
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ErgoTree {
    header: ErgoTreeHeader,
    tree: Result<ParsedTree, ErgoTreeConstantsParsingError>,
}

#[derive(PartialEq, Eq, Debug, Clone)]
struct ErgoTreeHeader(u8);

impl ErgoTreeHeader {
    const CONSTANT_SEGREGATION_FLAG: u8 = 0x10;

    pub fn is_constant_segregation(&self) -> bool {
        self.0 & ErgoTreeHeader::CONSTANT_SEGREGATION_FLAG != 0
    }
}

/// Whole ErgoTree parsing (deserialization) error
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ErgoTreeConstantsParsingError {
    /// Ergo tree bytes (faild to deserialize)
    pub bytes: Vec<u8>,
    /// Deserialization error
    pub error: SerializationError,
}

/// ErgoTree root expr parsing (deserialization) error
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct ErgoTreeRootParsingError {
    /// Ergo tree root expr bytes (faild to deserialize)
    pub bytes: Vec<u8>,
    /// Deserialization error
    pub error: SerializationError,
}

/// ErgoTree parsing (deserialization) error
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ErgoTreeParsingError {
    /// Whole ErgoTree parsing (deserialization) error
    TreeParsingError(ErgoTreeConstantsParsingError),
    /// ErgoTree root expr parsing (deserialization) error
    RootParsingError(ErgoTreeRootParsingError),
}

impl ErgoTree {
    const DEFAULT_HEADER: ErgoTreeHeader = ErgoTreeHeader(0);

    /// get Expr out of ErgoTree
    pub fn proposition(&self) -> Result<Rc<Expr>, ErgoTreeParsingError> {
        self.tree
            .clone()
            .map_err(ErgoTreeParsingError::TreeParsingError)
            .and_then(|t| t.root.map_err(ErgoTreeParsingError::RootParsingError))
    }

    /// Build ErgoTree using expr as is, without constants segregated
    pub fn without_segregation(expr: Rc<Expr>) -> ErgoTree {
        ErgoTree {
            header: ErgoTree::DEFAULT_HEADER,
            tree: Ok(ParsedTree {
                constants: Vec::new(),
                root: Ok(expr),
            }),
        }
    }

    /// Build ErgoTree with constants segregated from expr
    pub fn with_segregation(_: Rc<Expr>) -> ErgoTree {
        todo!()
    }
}

impl From<Rc<Expr>> for ErgoTree {
    fn from(expr: Rc<Expr>) -> Self {
        match expr.as_ref() {
            Expr::Const(Constant { tpe, .. }) if *tpe == SType::SSigmaProp => {
                ErgoTree::without_segregation(expr)
            }
            _ => ErgoTree::with_segregation(expr),
        }
    }
}
impl SigmaSerializable for ErgoTreeHeader {
    fn sigma_serialize<W: WriteSigmaVlqExt>(&self, w: &mut W) -> Result<(), io::Error> {
        w.put_u8(self.0)?;
        Ok(())
    }
    fn sigma_parse<R: ReadSigmaVlqExt>(r: &mut R) -> Result<Self, SerializationError> {
        let header = r.get_u8()?;
        Ok(ErgoTreeHeader(header))
    }
}

impl SigmaSerializable for ErgoTree {
    fn sigma_serialize<W: WriteSigmaVlqExt>(&self, w: &mut W) -> Result<(), io::Error> {
        self.header.sigma_serialize(w)?;
        match &self.tree {
            Ok(ParsedTree { constants, root }) => {
                if self.header.is_constant_segregation() {
                    w.put_usize_as_u32(constants.len())?;
                    assert!(
                        constants.is_empty(),
                        "separate constants serialization is not yet supported"
                    );
                }
                match root {
                    Ok(expr) => expr.sigma_serialize(w)?,
                    Err(ErgoTreeRootParsingError { bytes, .. }) => w.write_all(&bytes[..])?,
                }
            }
            Err(ErgoTreeConstantsParsingError { bytes, .. }) => w.write_all(&bytes[..])?,
        }
        Ok(())
    }

    fn sigma_parse<R: ReadSigmaVlqExt>(r: &mut R) -> Result<Self, SerializationError> {
        let header = ErgoTreeHeader::sigma_parse(r)?;
        if header.is_constant_segregation() {
            let constants_len = r.get_u32()?;
            if constants_len != 0 {
                return Err(SerializationError::NotImplementedYet(
                    "separate constants serialization is not yet supported".to_string(),
                ));
            }
        }
        let constants = Vec::new();
        let root = Expr::sigma_parse(r)?;
        Ok(ErgoTree {
            header,
            tree: Ok(ParsedTree {
                constants,
                root: Ok(Rc::new(root)),
            }),
        })
    }

    fn sigma_parse_bytes(mut bytes: Vec<u8>) -> Result<Self, SerializationError> {
        let cursor = Cursor::new(&mut bytes[..]);
        let mut r = PeekableReader::new(cursor);
        let header = ErgoTreeHeader::sigma_parse(&mut r)?;
        if header.is_constant_segregation() {
            let constants_len = r.get_u32()?;
            if constants_len != 0 {
                return Ok(ErgoTree {
                    header,
                    tree: Err(ErgoTreeConstantsParsingError {
                        bytes: bytes[1..].to_vec(),
                        error: SerializationError::NotImplementedYet(
                            "separate constants serialization is not yet supported".to_string(),
                        ),
                    }),
                });
            }
        }
        let constants = Vec::new();
        let mut rest_of_the_bytes = Vec::new();
        let _ = r.read_to_end(&mut rest_of_the_bytes);
        let rest_of_the_bytes_copy = rest_of_the_bytes.clone();
        match Expr::sigma_parse_bytes(rest_of_the_bytes) {
            Ok(parsed) => Ok(ErgoTree {
                header,
                tree: Ok(ParsedTree {
                    constants,
                    root: Ok(Rc::new(parsed)),
                }),
            }),
            Err(err) => Ok(ErgoTree {
                header,
                tree: Ok(ParsedTree {
                    constants,
                    root: Err(ErgoTreeRootParsingError {
                        bytes: rest_of_the_bytes_copy,
                        error: err,
                    }),
                }),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ast::ConstantVal, chain, sigma_protocol::SigmaProp, types::SType};
    use proptest::prelude::*;
    use sigma_ser::test_helpers::*;

    impl Arbitrary for ErgoTree {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            (any::<SigmaProp>())
                .prop_map(|p| {
                    ErgoTree::from(Rc::new(Expr::Const(Constant {
                        tpe: SType::SSigmaProp,
                        v: ConstantVal::SigmaProp(Box::new(p)),
                    })))
                })
                .boxed()
        }
    }

    proptest! {

        #[test]
        fn ser_roundtrip(v in any::<ErgoTree>()) {
            prop_assert_eq![sigma_serialize_roundtrip(&(v)), v];
        }
    }

    #[test]
    fn deserialization_non_parseable_tree_ok() {
        // constants length is set
        assert!(ErgoTree::sigma_parse_bytes(vec![0, 1]).is_ok());
    }

    #[test]
    fn deserialization_non_parseable_root_ok() {
        // constants length is zero, but Expr is invalid
        assert!(ErgoTree::sigma_parse_bytes(vec![0, 0, 1]).is_ok());
    }

    #[test]
    fn test_constant_segregation_header_flag_support() {
        let encoder = chain::AddressEncoder::new(chain::NetworkPrefix::Mainnet);
        let address = encoder
            .parse_address_from_str("9hzP24a2q8KLPVCUk7gdMDXYc7vinmGuxmLp5KU7k9UwptgYBYV")
            .unwrap();

        let contract = chain::Contract::pay_to_address(address).unwrap();
        let bytes = &contract.get_ergo_tree().sigma_serialise_bytes();
        assert_eq!(&bytes[..2], vec![0u8, 8u8].as_slice());
    }
}
