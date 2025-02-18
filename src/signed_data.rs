// Copyright 2015 Brian Smith.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

use crate::der::{self, FromDer};
use crate::error::Error;

/// X.509 certificates and related items that are signed are almost always
/// encoded in the format "tbs||signatureAlgorithm||signature". This structure
/// captures this pattern as an owned data type.
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
#[cfg(feature = "alloc")]
#[derive(Clone, Debug)]
pub(crate) struct OwnedSignedData {
    /// The signed data. This would be `tbsCertificate` in the case of an X.509
    /// certificate, `tbsResponseData` in the case of an OCSP response, `tbsCertList`
    /// in the case of a CRL, and the data nested in the `digitally-signed` construct for
    /// TLS 1.2 signed data.
    pub(crate) data: Vec<u8>,

    /// The value of the `AlgorithmIdentifier`. This would be
    /// `signatureAlgorithm` in the case of an X.509 certificate, OCSP
    /// response or CRL. This would have to be synthesized in the case of TLS 1.2
    /// signed data, since TLS does not identify algorithms by ASN.1 OIDs.
    pub(crate) algorithm: Vec<u8>,

    /// The value of the signature. This would be `signature` in an X.509
    /// certificate, OCSP response or CRL. This would be the value of
    /// `DigitallySigned.signature` for TLS 1.2 signed data.
    pub(crate) signature: Vec<u8>,
}

#[cfg(feature = "alloc")]
impl OwnedSignedData {
    /// Return a borrowed [`SignedData`] from the owned representation.
    pub(crate) fn borrow(&self) -> SignedData<'_> {
        SignedData {
            data: untrusted::Input::from(&self.data),
            algorithm: untrusted::Input::from(&self.algorithm),
            signature: untrusted::Input::from(&self.signature),
        }
    }
}

/// X.509 certificates and related items that are signed are almost always
/// encoded in the format "tbs||signatureAlgorithm||signature". This structure
/// captures this pattern.
#[derive(Debug)]
pub(crate) struct SignedData<'a> {
    /// The signed data. This would be `tbsCertificate` in the case of an X.509
    /// certificate, `tbsResponseData` in the case of an OCSP response, `tbsCertList`
    /// in the case of a CRL, and the data nested in the `digitally-signed` construct for
    /// TLS 1.2 signed data.
    pub(crate) data: untrusted::Input<'a>,

    /// The value of the `AlgorithmIdentifier`. This would be
    /// `signatureAlgorithm` in the case of an X.509 certificate, OCSP
    /// response or CRL. This would have to be synthesized in the case of TLS 1.2
    /// signed data, since TLS does not identify algorithms by ASN.1 OIDs.
    pub(crate) algorithm: untrusted::Input<'a>,

    /// The value of the signature. This would be `signature` in an X.509
    /// certificate, OCSP response or CRL. This would be the value of
    /// `DigitallySigned.signature` for TLS 1.2 signed data.
    pub(crate) signature: untrusted::Input<'a>,
}

impl<'a> SignedData<'a> {
    /// Parses the concatenation of "tbs||signatureAlgorithm||signature" that
    /// is common in the X.509 certificate and OCSP response syntaxes.
    ///
    /// X.509 Certificates (RFC 5280) look like this:
    ///
    /// ```ASN.1
    /// Certificate (SEQUENCE) {
    ///     tbsCertificate TBSCertificate,
    ///     signatureAlgorithm AlgorithmIdentifier,
    ///     signatureValue BIT STRING
    /// }
    ///
    /// OCSP responses (RFC 6960) look like this:
    /// ```ASN.1
    /// BasicOCSPResponse {
    ///     tbsResponseData ResponseData,
    ///     signatureAlgorithm AlgorithmIdentifier,
    ///     signature BIT STRING,
    ///     certs [0] EXPLICIT SEQUENCE OF Certificate OPTIONAL
    /// }
    /// ```
    ///
    /// Note that this function does NOT parse the outermost `SEQUENCE` or the
    /// `certs` value.
    ///
    /// The return value's first component is the contents of
    /// `tbsCertificate`/`tbsResponseData`; the second component is a `SignedData`
    /// structure that can be passed to `verify_signed_data`.
    ///
    /// The provided size_limit will enforce the largest possible outermost `SEQUENCE` this
    /// function will read.
    pub(crate) fn from_der(
        der: &mut untrusted::Reader<'a>,
        size_limit: usize,
    ) -> Result<(untrusted::Input<'a>, Self), Error> {
        let (data, tbs) = der.read_partial(|input| {
            der::expect_tag_and_get_value_limited(input, der::Tag::Sequence, size_limit)
        })?;
        let algorithm = der::expect_tag_and_get_value(der, der::Tag::Sequence)?;
        let signature = der::bit_string_with_no_unused_bits(der)?;

        Ok((
            tbs,
            SignedData {
                data,
                algorithm,
                signature,
            },
        ))
    }

    /// Convert the borrowed signed data to an [`OwnedSignedData`].
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
    pub(crate) fn to_owned(&self) -> OwnedSignedData {
        OwnedSignedData {
            data: self.data.as_slice_less_safe().to_vec(),
            algorithm: self.algorithm.as_slice_less_safe().to_vec(),
            signature: self.signature.as_slice_less_safe().to_vec(),
        }
    }
}

/// Verify `signed_data` using the public key in the DER-encoded
/// SubjectPublicKeyInfo `spki` using one of the algorithms in
/// `supported_algorithms`.
///
/// The algorithm is chosen based on the algorithm information encoded in the
/// algorithm identifiers in `public_key` and `signed_data.algorithm`. The
/// ordering of the algorithms in `supported_algorithms` does not really matter,
/// but generally more common algorithms should go first, as it is scanned
/// linearly for matches.
pub(crate) fn verify_signed_data(
    supported_algorithms: &[&dyn SignatureVerificationAlgorithm],
    spki_value: untrusted::Input,
    signed_data: &SignedData,
) -> Result<(), Error> {
    // We need to verify the signature in `signed_data` using the public key
    // in `public_key`. In order to know which *ring* signature verification
    // algorithm to use, we need to know the public key algorithm (ECDSA,
    // RSA PKCS#1, etc.), the curve (if applicable), and the digest algorithm.
    // `signed_data` identifies only the public key algorithm and the digest
    // algorithm, and `public_key` identifies only the public key algorithm and
    // the curve (if any). Thus, we have to combine information from both
    // inputs to figure out which `ring::signature::VerificationAlgorithm` to
    // use to verify the signature.
    //
    // This is all further complicated by the fact that we don't have any
    // implicit knowledge about any algorithms or identifiers, since all of
    // that information is encoded in `supported_algorithms.` In particular, we
    // avoid hard-coding any of that information so that (link-time) dead code
    // elimination will work effectively in eliminating code for unused
    // algorithms.

    // Parse the signature.
    //
    let mut found_signature_alg_match = false;
    for supported_alg in supported_algorithms.iter().filter(|alg| {
        alg.signature_alg_id()
            .matches_algorithm_id_value(signed_data.algorithm)
    }) {
        match verify_signature(
            *supported_alg,
            spki_value,
            signed_data.data,
            signed_data.signature,
        ) {
            Err(Error::UnsupportedSignatureAlgorithmForPublicKey) => {
                found_signature_alg_match = true;
                continue;
            }
            result => {
                return result;
            }
        }
    }

    if found_signature_alg_match {
        Err(Error::UnsupportedSignatureAlgorithmForPublicKey)
    } else {
        Err(Error::UnsupportedSignatureAlgorithm)
    }
}

pub(crate) fn verify_signature(
    signature_alg: &dyn SignatureVerificationAlgorithm,
    spki_value: untrusted::Input,
    msg: untrusted::Input,
    signature: untrusted::Input,
) -> Result<(), Error> {
    let spki = spki_value.read_all(Error::BadDer, SubjectPublicKeyInfo::from_der)?;
    if !signature_alg
        .public_key_alg_id()
        .matches_algorithm_id_value(spki.algorithm_id_value)
    {
        return Err(Error::UnsupportedSignatureAlgorithmForPublicKey);
    }

    signature_alg
        .verify_signature(
            spki.key_value.as_slice_less_safe(),
            msg.as_slice_less_safe(),
            signature.as_slice_less_safe(),
        )
        .map_err(|_| Error::InvalidSignatureForPublicKey)
}

struct SubjectPublicKeyInfo<'a> {
    algorithm_id_value: untrusted::Input<'a>,
    key_value: untrusted::Input<'a>,
}

impl<'a> FromDer<'a> for SubjectPublicKeyInfo<'a> {
    // Parse the public key into an algorithm OID, an optional curve OID, and the
    // key value. The caller needs to check whether these match the
    // `PublicKeyAlgorithm` for the `SignatureVerificationAlgorithm` that is matched when
    // parsing the signature.
    fn from_der(reader: &mut untrusted::Reader<'a>) -> Result<Self, Error> {
        let algorithm_id_value = der::expect_tag_and_get_value(reader, der::Tag::Sequence)?;
        let key_value = der::bit_string_with_no_unused_bits(reader)?;
        Ok(SubjectPublicKeyInfo {
            algorithm_id_value,
            key_value,
        })
    }
}

/// An abstract signature verification algorithm.
///
/// One of these is needed per supported pair of public key type (identified
/// with `public_key_alg_id()`) and `signatureAlgorithm` (identified with
/// `signature_alg_id()`).  Note that both of these `AlgorithmIdentifier`s include
/// the parameters encoding, so separate `SignatureVerificationAlgorithm`s are needed
/// for each possible public key or signature parameters.
pub trait SignatureVerificationAlgorithm: Send + Sync {
    /// Return the `AlgorithmIdentifier` that must be present on a `subjectPublicKeyInfo`
    /// for this `SignatureVerificationAlgorithm` to be considered for verification.
    fn public_key_alg_id(&self) -> alg_id::AlgorithmIdentifier;

    /// Return the `AlgorithmIdentifier` that must be present as the `signatureAlgorithm`
    /// on the data to be verified for this `SignatureVerificationAlgorithm` to be considered
    /// for this `SignatureVerificationAlgorithm` to be considered.
    fn signature_alg_id(&self) -> alg_id::AlgorithmIdentifier;

    /// Verify a signature.
    ///
    /// `public_key` is the `subjectPublicKey` value from a `SubjectPublicKeyInfo` encoding
    ///  and is untrusted.
    ///
    ///  `message` is the data over which the signature was allegedly computed.
    ///  It is not hashed; implementations of this trait function must do hashing
    ///  if that is required by the algorithm they implement.
    ///
    ///  `signature` is the signature allegedly over `message`.
    ///
    /// Return `Ok(())` only if `signature` is a valid signature on `message`.
    ///
    /// Return `Err(InvalidSignature)` if the signature is invalid, including if the `public_key`
    /// encoding is invalid.  There is no need or opportunity to produce errors
    /// that are more specific than this.
    fn verify_signature(
        &self,
        public_key: &[u8],
        message: &[u8],
        signature: &[u8],
    ) -> Result<(), InvalidSignature>;
}

/// A detail-less error when a signature is not valid.
#[derive(Debug, Copy, Clone)]
pub struct InvalidSignature;

/// Encodings of the PKIX AlgorithmIdentifier type:
///
/// ```ASN.1
/// AlgorithmIdentifier  ::=  SEQUENCE  {
///     algorithm               OBJECT IDENTIFIER,
///     parameters              ANY DEFINED BY algorithm OPTIONAL  }
///                                -- contains a value of the type
///                                -- registered for use with the
///                                -- algorithm object identifier value
/// ```
/// (from <https://www.rfc-editor.org/rfc/rfc5280#section-4.1.1.2>)
///
/// The outer sequence encoding is not included, so this is an OID encoding
/// for `algorithm` plus the `parameters` value.
///
/// This module contains a set of common values, and exists to keep the
/// names of these separate from the actual algorithm implementations.
pub mod alg_id {
    /// A `AlgorithmIdentifier` encoding.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AlgorithmIdentifier {
        asn1_id_value: untrusted::Input<'static>,
    }

    impl AlgorithmIdentifier {
        /// Makes a new `AlgorithmIdentifier` from a static octet string.
        ///
        /// This does not validate the contents of the string.
        pub const fn new(bytes: &'static [u8]) -> Self {
            Self {
                asn1_id_value: untrusted::Input::from(bytes),
            }
        }

        pub(crate) fn matches_algorithm_id_value(&self, encoded: untrusted::Input) -> bool {
            encoded == self.asn1_id_value
        }
    }

    // See src/data/README.md.

    /// AlgorithmIdentifier for `id-ecPublicKey` with named curve `secp256r1`.
    pub const ECDSA_P256: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-ecdsa-p256.der"));

    /// AlgorithmIdentifier for `id-ecPublicKey` with named curve `secp384r1`.
    pub const ECDSA_P384: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-ecdsa-p384.der"));

    /// AlgorithmIdentifier for `ecdsa-with-SHA256`.
    pub const ECDSA_SHA256: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-ecdsa-sha256.der"));

    /// AlgorithmIdentifier for `ecdsa-with-SHA384`.
    pub const ECDSA_SHA384: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-ecdsa-sha384.der"));

    /// AlgorithmIdentifier for `rsaEncryption`.
    pub const RSA_ENCRYPTION: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-encryption.der"));

    /// AlgorithmIdentifier for `sha256WithRSAEncryption`.
    pub const RSA_PKCS1_SHA256: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pkcs1-sha256.der"));

    /// AlgorithmIdentifier for `sha384WithRSAEncryption`.
    pub const RSA_PKCS1_SHA384: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pkcs1-sha384.der"));

    /// AlgorithmIdentifier for `sha512WithRSAEncryption`.
    pub const RSA_PKCS1_SHA512: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pkcs1-sha512.der"));

    /// AlgorithmIdentifier for `rsassaPss` with:
    ///
    /// - hashAlgorithm: sha256
    /// - maskGenAlgorithm: mgf1 with sha256
    /// - saltLength: 32
    pub const RSA_PSS_SHA256: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pss-sha256.der"));

    /// AlgorithmIdentifier for `rsassaPss` with:
    ///
    /// - hashAlgorithm: sha384
    /// - maskGenAlgorithm: mgf1 with sha384
    /// - saltLength: 48
    pub const RSA_PSS_SHA384: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pss-sha384.der"));

    /// AlgorithmIdentifier for `rsassaPss` with:
    ///
    /// - hashAlgorithm: sha512
    /// - maskGenAlgorithm: mgf1 with sha512
    /// - saltLength: 64
    pub const RSA_PSS_SHA512: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-rsa-pss-sha512.der"));

    /// AlgorithmIdentifier for `ED25519`.
    pub const ED25519: AlgorithmIdentifier =
        AlgorithmIdentifier::new(include_bytes!("data/alg-ed25519.der"));

    #[test]
    fn test_algorithm_identifer() {
        let id = AlgorithmIdentifier::new(&[1, 2, 3]);
        #[allow(clippy::clone_on_copy)]
        let _ = id.clone();
        let _ = id;
        assert!(format!("{:?}", id).starts_with("AlgorithmIdentifier "));
    }
}
