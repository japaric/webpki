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

//! webpki: Web PKI X.509 Certificate Validation.
//!
//! See `EndEntityCert`'s documentation for a description of the certificate
//! processing steps necessary for a TLS connection.
//!
//! # Features
//!
//! | Feature | Description |
//! | ------- | ----------- |
//! | `alloc` | Enable features that require use of the heap. Currently all RSA signature algorithms require this feature. |
//! | `std` | Enable features that require libstd. Implies `alloc`. |
//! | `ring` | Enable use of the *ring* crate for cryptography. |

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(unreachable_pub)]
#![deny(warnings, missing_docs, clippy::as_conversions)]
#![allow(
    clippy::len_without_is_empty,
    clippy::new_without_default,
    clippy::single_match,
    clippy::single_match_else,
    clippy::type_complexity,
    clippy::upper_case_acronyms
)]
// Enable documentation for all features on docs.rs
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(any(test, feature = "alloc"))]
#[cfg_attr(test, macro_use)]
extern crate alloc;

#[macro_use]
mod der;

mod cert;
mod end_entity;
mod error;
#[cfg(feature = "ring")]
mod ring_algs;
mod signed_data;
mod subject_name;
mod time;
mod trust_anchor;

mod crl;
mod verify_cert;
mod x509;

pub use {
    cert::{Cert, EndEntityOrCa},
    crl::{BorrowedCertRevocationList, BorrowedRevokedCert, CertRevocationList, RevocationReason},
    end_entity::EndEntityCert,
    error::Error,
    signed_data::{alg_id, InvalidSignature, SignatureVerificationAlgorithm},
    subject_name::{
        AddrParseError, DnsNameRef, InvalidDnsNameError, InvalidSubjectNameError, IpAddrRef,
        SubjectNameRef,
    },
    time::Time,
    trust_anchor::TrustAnchor,
    verify_cert::KeyUsage,
};

#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
#[cfg(feature = "alloc")]
pub use {
    crl::{OwnedCertRevocationList, OwnedRevokedCert},
    subject_name::{DnsName, IpAddr},
};

#[cfg_attr(docsrs, doc(cfg(feature = "ring")))]
#[cfg(feature = "ring")]
pub use ring_algs::{
    ECDSA_P256_SHA256, ECDSA_P256_SHA384, ECDSA_P384_SHA256, ECDSA_P384_SHA384, ED25519,
};

#[cfg_attr(docsrs, doc(cfg(all(feature = "ring", feature = "alloc"))))]
#[cfg(all(feature = "ring", feature = "alloc"))]
pub use ring_algs::{
    RSA_PKCS1_2048_8192_SHA256, RSA_PKCS1_2048_8192_SHA384, RSA_PKCS1_2048_8192_SHA512,
    RSA_PKCS1_3072_8192_SHA384, RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
    RSA_PSS_2048_8192_SHA384_LEGACY_KEY, RSA_PSS_2048_8192_SHA512_LEGACY_KEY,
};
