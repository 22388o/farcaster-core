//! Cryptographic types (keys, signatures, commitments, etc) and traits (commit, generate key,
//! sign, etc) used to create the generic framework for supporting multiple blockchains under the
//! same interface.

use std::convert::TryInto;
use std::error;
use std::fmt::{self, Debug};
use std::io;

use thiserror::Error;
use tiny_keccak::{Hasher, Keccak};

use crate::consensus::{self, CanonicalBytes, Decodable, Encodable};
use crate::role::Accordant;

#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub mod dleq;
#[cfg(feature = "experimental")]
#[cfg_attr(docsrs, doc(cfg(feature = "experimental")))]
pub mod slip10;

/// List of cryptographic errors that can be encountered in cryptographic operations such as
/// signatures, proofs, key derivation, or commitments.
#[derive(Error, Debug)]
pub enum Error {
    /// The key identifier is not supported and the key cannot be derived.
    #[error("The key identifier is not supported and the key cannot be derived")]
    UnsupportedKey,
    /// The key or key identifier does not exists or is missing.
    #[error("The key or key identifier does not exists or is missing")]
    MissingKey,
    /// The signature does not pass the validation tests.
    #[error("The signature does not pass the validation")]
    InvalidSignature,
    /// The adaptor key is not valid.
    #[error("The adaptor key is not valid")]
    InvalidAdaptorKey,
    /// The adaptor signature does not pass the validation tests.
    #[error("The adaptor signature does not pass the validation")]
    InvalidEncryptedSignature,
    /// The proof does not pass the validation tests.
    #[error("The proof does not pass the validation")]
    InvalidProof,
    /// The commitment does not match the given value.
    #[error("The commitment does not match the given value")]
    InvalidCommitment,
    /// The Pedersen commitment does not match the given value.
    #[error("The Pedersen commitment does not match the given value")]
    InvalidPedersenCommitment,
    /// The ring signature does not recompute.
    #[error("The ring signature does not recompute")]
    InvalidRingSignature,
    /// The proof of knowledge signature is invalid.
    #[error("The proof of knowledge signature is invalid")]
    InvalidProofOfKnowledge,
    /// SLIP10 error when manipulating extended secret keys.
    #[error("SLIP10 error: {0}")]
    Slip10(#[from] slip10::Error),
    /// Any cryptographic error not part of this list.
    #[error("Cryptographic error: {0}")]
    Other(Box<dyn error::Error + Send + Sync>),
}

impl Error {
    /// Creates a new cryptographic error of type [`Self::Other`] with an arbitrary payload.
    pub fn new<E>(error: E) -> Self
    where
        E: Into<Box<dyn error::Error + Send + Sync>>,
    {
        Self::Other(error.into())
    }

    /// Consumes the `Error`, returning its inner error (if any).
    ///
    /// If this [`enum@Error`] was constructed via [`new`] then this function will return [`Some`],
    /// otherwise it will return [`None`].
    ///
    /// [`new`]: Error::new
    ///
    pub fn into_inner(self) -> Option<Box<dyn error::Error + Send + Sync>> {
        match self {
            Self::Other(error) => Some(error),
            _ => None,
        }
    }
}

/// Element `E` prefixed with a tag `T`. Used to tag content with some ids. Tag needs `Eq` to be
/// used in vectors or sets and identify the content. Tags can be [`ArbitratingKeyId`],
/// [`AccordantKeyId`] or any other type of identifiers.
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub struct TaggedElement<T, E>
where
    T: Eq,
{
    tag: T,
    elem: E,
}

impl<T, E> TaggedElement<T, E>
where
    T: Eq,
{
    /// Create a new tagged element `E` with the tag `T`.
    pub fn new(tag: T, elem: E) -> Self {
        Self { tag, elem }
    }

    /// Returns the tag `T`.
    pub fn tag(&self) -> &T {
        &self.tag
    }

    /// Returns the element `E`.
    pub fn elem(&self) -> &E {
        &self.elem
    }
}

impl<T, E> fmt::Display for TaggedElement<T, E>
where
    T: Eq + fmt::Display,
    E: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<{}: {}>", self.tag, self.elem)
    }
}

impl<T, E> Encodable for TaggedElement<T, E>
where
    T: Eq + Encodable,
    E: CanonicalBytes,
{
    #[inline]
    fn consensus_encode<S: io::Write>(&self, s: &mut S) -> Result<usize, io::Error> {
        let len = self.tag.consensus_encode(s)?;
        Ok(len + self.elem.as_canonical_bytes().consensus_encode(s)?)
    }
}

impl<T, E> Decodable for TaggedElement<T, E>
where
    T: Eq + Decodable,
    E: CanonicalBytes,
{
    #[inline]
    fn consensus_decode<D: io::Read>(d: &mut D) -> Result<Self, consensus::Error> {
        let tag = T::consensus_decode(d)?;
        let elem = E::from_canonical_bytes(unwrap_vec_ref!(d).as_ref())?;
        Ok(TaggedElement { tag, elem })
    }
}

/// A vector of `T` tagged elements `E`.
pub type TaggedElements<T, E> = Vec<TaggedElement<T, E>>;

/// A vector of [`u16`] tagged keys of type `E`.
pub type TaggedExtraKeys<E> = Vec<TaggedElement<u16, E>>;

/// A vector of shared keys tagged with [`SharedKeyId`] of type `E`.
pub type TaggedSharedKeys<E> = Vec<TaggedElement<SharedKeyId, E>>;

/// List of all possible arbitrating keys as defined for the base protocol in the RFCs. Extra keys
/// can be defined with [`Self::Extra`] variant and an `u16` identifier. Those keys can be used for
/// extra off-chain protocol such as multi-signature or multi-party computation schemes.
#[derive(Debug, Clone, Copy, Display)]
#[display(Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub enum ArbitratingKeyId {
    /// Arbitrating key used to fund the [`Lockable`] transaction through [`Fundable`].
    ///
    /// [`Lockable`]: crate::transaction::Lockable
    /// [`Fundable`]: crate::transaction::Fundable
    Lock,
    /// Key used in the [`Buyable`] transaction.
    ///
    /// [`Buyable`]: crate::transaction::Buyable
    Buy,
    /// Key used in the [`Cancelable`] transaction.
    ///
    /// [`Cancelable`]: crate::transaction::Cancelable
    Cancel,
    /// Key used in the [`Refundable`] transaction.
    ///
    /// [`Refundable`]: crate::transaction::Refundable
    Refund,
    /// Key used in the [`Punishable`] transaction.
    ///
    /// [`Punishable`]: crate::transaction::Punishable
    Punish,
    /// Any other key used for extra off-chain protocol such as multi-signature or multi-party
    /// computation schemes. Contains its own identifier. The identifier must not conflict with
    /// defined identifiers in RFCs.
    Extra(u16),
}

/// Defines the base accordant key identifier [`Self::Spend`] and all possible extra keys with
/// [`Self::Extra`] variant containing the `u16` identifier.
#[derive(Debug, Clone, Copy, Display)]
#[display(Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub enum AccordantKeyId {
    /// Accordant bought/sold key over the arbitrating blockchain.
    Spend,
    /// Any other key needed in the context of an accordant blockchain. Contains its own
    /// identifier.  The identifier must not conflict with defined identifiers in RFCs.
    Extra(u16),
}

/// Identifier for shared private keys over the arbitrating and accordant blockchains. E.g. the
/// `view` key needed to parse the Monero blockchain is a shared private key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[display(Debug)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(crate = "serde_crate")
)]
pub struct SharedKeyId(u16);

impl SharedKeyId {
    /// Create a new shared key identifier.
    pub fn new(id: u16) -> Self {
        Self(id)
    }

    /// Return the identifier value.
    pub fn id(&self) -> u16 {
        self.0
    }
}

impl Encodable for SharedKeyId {
    #[inline]
    fn consensus_encode<S: io::Write>(&self, s: &mut S) -> Result<usize, io::Error> {
        self.0.consensus_encode(s)
    }
}

impl Decodable for SharedKeyId {
    #[inline]
    fn consensus_decode<D: io::Read>(d: &mut D) -> Result<Self, consensus::Error> {
        Ok(Self(u16::consensus_decode(d)?))
    }
}

/// The list of possible [`Accordant`] keys (secret and public) a swap role has after reveal.
pub struct AccordantKeys<A: Accordant> {
    /// The accordant spend public key.
    pub spend_key: A::PublicKey,
    /// A list of extra accordant public keys.
    pub extra_accordant_keys: Vec<TaggedElement<u16, A::PublicKey>>,
    /// A list of secret shared keys, e.g. shared view keys in non-transparent blockchains.
    pub shared_keys: Vec<TaggedElement<SharedKeyId, A::SharedSecretKey>>,
}

/// The list of all accordant keys swap roles have after reveal.
pub struct SwapAccordantKeys<A: Accordant> {
    /// Alice's accordant keys (secret and public).
    pub alice: AccordantKeys<A>,
    /// Bob's accordant keys (secret and public).
    pub bob: AccordantKeys<A>,
}

fixed_hash::construct_fixed_hash!(
    /// Result of a keccak256 commitment.
    #[cfg_attr(
        feature = "serde",
        derive(Serialize, Deserialize),
        serde(crate = "serde_crate"),
    )]
    pub struct KeccakCommitment(32);
);

impl KeccakCommitment {
    /// Create a null commitment hash with all zeros.
    pub fn null_hash() -> Self {
        Self([0u8; 32])
    }

    /// Hash a stream of bytes with the Keccak-256 hash function.
    pub fn new(input: [u8; 32]) -> Self {
        Self(input)
    }
}

impl CanonicalBytes for KeccakCommitment {
    fn as_canonical_bytes(&self) -> Vec<u8> {
        (*self).to_fixed_bytes().into()
    }

    fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, consensus::Error>
    where
        Self: Sized,
    {
        Ok(Self::new(bytes.try_into().map_err(consensus::Error::new)?))
    }
}

/// Engine to produce and validate hash commitments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitmentEngine;

impl Commit<KeccakCommitment> for CommitmentEngine {
    fn commit_to<T: AsRef<[u8]>>(&self, value: T) -> KeccakCommitment {
        let mut out = [0u8; 32];
        let mut keccak = Keccak::v256();
        keccak.update(value.as_ref());
        keccak.finalize(&mut out);
        KeccakCommitment::new(out)
    }
}

/// Required for [`Arbitrating`] and [`Accordant`] blockchains to fix the cryptographic secret key
/// and public key types. The public key type is shared across the network and used in
/// transactions, the secret key type is used during signing operation, proofs, etc.
///
/// [`Arbitrating`]: crate::role::Arbitrating
/// [`Accordant`]: crate::role::Accordant
pub trait Keys {
    /// Secret key type used for signing and proving.
    type SecretKey;

    /// Public key type used in transactions.
    type PublicKey: Clone + PartialEq + Debug + fmt::Display + CanonicalBytes;

    /// Return a list of extra public key identifiers to use during the setup phase.
    fn extra_keys() -> Vec<u16>;
}

/// Required for [`Arbitrating`] and [`Accordant`] blockchains to fix the potential shared secret
/// keys send over the network. E.g. the private `view` key needed to parse the Monero blockchain.
///
/// [`Arbitrating`]: crate::role::Arbitrating
/// [`Accordant`]: crate::role::Accordant
pub trait SharedSecretKeys {
    /// Shareable secret key type used to parse, e.g., non-transparent blockchain.
    type SharedSecretKey: Clone + PartialEq + Debug + CanonicalBytes;

    /// Return a list of extra shared secret key identifiers to use during the setup phase.
    fn shared_keys() -> Vec<SharedKeyId>;
}

/// Trait required for [`Arbitrating`] blockchains to define the cryptographic message format to
/// sign, the signature format and adaptor signature format used in the cryptographic operations
/// such as signing and verifying signatures and adaptor signatures.
///
/// [`Arbitrating`]: crate::role::Arbitrating
pub trait Signatures {
    /// Type of the message passed to sign or adaptor sign methods, transactions will produce
    /// messages that will be passed to these methods.
    type Message: Clone + Debug;

    /// Defines the signature format for the arbitrating blockchain.
    type Signature: Clone + Debug + fmt::Display + CanonicalBytes;

    /// Defines the adaptor signature format for the arbitrating blockchain. Adaptor signature may
    /// have a different format from the signature depending on the cryptographic primitives used.
    type EncryptedSignature: Clone + Debug + fmt::Display + CanonicalBytes;
}

/// Meta trait regrouping all the needed trait combinations a key manager must implement to manage
/// all the keys needed when executing the protocol on [`Alice`] and [`Bob`] methods. This trait is
/// auto-implemented for all `T` meeting the requirements.
///
/// [`Alice`]: crate::role::Alice
/// [`Bob`]: crate::role::Bob
pub trait KeyGenerator<ArPublicKey, AcPublicKey, ArSharedKey, AcSharedKey, Proof>:
    GenerateKey<ArPublicKey, ArbitratingKeyId>
    + GenerateKey<AcPublicKey, AccordantKeyId>
    + ProveCrossGroupDleq<ArPublicKey, AcPublicKey, Proof>
    + GenerateSharedKey<ArSharedKey>
    + GenerateSharedKey<AcSharedKey>
{
}

impl<T, ArPublicKey, AcPublicKey, ArSharedKey, AcSharedKey, Proof>
    KeyGenerator<ArPublicKey, AcPublicKey, ArSharedKey, AcSharedKey, Proof> for T
where
    T: GenerateKey<ArPublicKey, ArbitratingKeyId>
        + GenerateKey<AcPublicKey, AccordantKeyId>
        + GenerateSharedKey<ArSharedKey>
        + GenerateSharedKey<AcSharedKey>
        + ProveCrossGroupDleq<ArPublicKey, AcPublicKey, Proof>,
{
}

/// Public key generator. Generic interface over `PublicKey`, the public key type, and `KeyId`, the
/// identifier, used to retreive public keys by their identifiers.
pub trait GenerateKey<PublicKey, KeyId> {
    /// Retreive a specific public key by its key id. If the key cannot be derived the
    /// implementation must return an [`Error::UnsupportedKey`], otherwise `Ok(PublicKey)` is
    /// returned.
    fn get_pubkey(&mut self, key_id: KeyId) -> Result<PublicKey, Error>;

    /// Return a vector of public keys matching the vector of key ids. Errors on the first key that
    /// can't be derived and return an [`Error::UnsupportedKey`].
    fn get_pubkeys(&mut self, key_ids: Vec<KeyId>) -> Result<Vec<PublicKey>, Error> {
        key_ids.into_iter().map(|id| self.get_pubkey(id)).collect()
    }
}

/// Private shared key generator. Generic interface over `SharedKey`, the private key type, used to
/// retreive private shared keys by their identifiers: [`SharedKeyId`].
pub trait GenerateSharedKey<SharedKey> {
    /// Retreive a specific shared private key by its key id. If the key cannot be derived the
    /// implementation must return an [`Error::UnsupportedKey`].
    fn get_shared_key(&mut self, key_id: SharedKeyId) -> Result<SharedKey, Error>;
}

// TODO give extra keys and/or shared keys in signing methods

/// Signature and encrypted signature generator and verifier. Produce and verify signatures and
/// encrypted sigantures based on public keys/key identifiers. Recover the private key through the
/// complete encrypted/decrypted signature.
pub trait Sign<PublicKey, SecretKey, Message, Signature, EncryptedSignature> {
    /// Sign the message with the corresponding private key identified by the provided arbitrating
    /// key identifier.
    fn sign(&mut self, key: ArbitratingKeyId, msg: Message) -> Result<Signature, Error>;

    /// Verify a signature for a given message with the provided public key.
    fn verify_signature(&self, key: &PublicKey, msg: Message, sig: &Signature)
        -> Result<(), Error>;

    /// Sign the message with the corresponding private key identified by the provided arbitrating
    /// key identifier and encrypt it (create an adaptor signature) with the provided encryption
    /// public key.
    fn encrypt_sign(
        &mut self,
        signing_key: ArbitratingKeyId,
        encryption_key: &PublicKey,
        msg: Message,
    ) -> Result<EncryptedSignature, Error>;

    /// Verify an encrypted signature for a given message with the provided signing public key and
    /// the public encryption key.
    fn verify_encrypted_signature(
        &self,
        signing_key: &PublicKey,
        encryption_key: &PublicKey,
        msg: Message,
        sig: &EncryptedSignature,
    ) -> Result<(), Error>;

    /// Decrypt the encrypted signature with the corresponding decryption private key identified by
    /// the provided accordant key identifier, producing a valid regular signature.
    fn decrypt_signature(
        &mut self,
        decryption_key: AccordantKeyId,
        sig: EncryptedSignature,
    ) -> Result<Signature, Error>;

    /// Recover the encryption key based on the encrypted signature, the encryption public key, and
    /// the regular (decrypted) signature.
    fn recover_secret_key(
        &self,
        encrypted_sig: EncryptedSignature,
        encryption_key: &PublicKey,
        sig: Signature,
    ) -> SecretKey;
}

/// Commitment generator and verifier. Generated commitments can be validated against candidates,
/// if correct the commit/reveal process is validated.
pub trait Commit<Commitment: Eq> {
    /// Provides a generic method to commit to any value referencable as stream of bytes.
    fn commit_to<T: AsRef<[u8]>>(&self, value: T) -> Commitment;

    /// Validate the equality between a candidate and a commitment, return `Ok(())` if the value
    /// commits to the same commitment's candidate, return [`Error::InvalidCommitment`]
    /// otherwise.
    fn validate<T: AsRef<[u8]>>(&self, candidate: T, commitment: Commitment) -> Result<(), Error> {
        if self.commit_to(candidate) == commitment {
            Ok(())
        } else {
            Err(Error::InvalidCommitment)
        }
    }
}

/// Proof generator and verifier for the cross-group projection of the accordant public spend key
/// as an arbitrating key used to encrypt signatures.
pub trait ProveCrossGroupDleq<EncryptionKey, AccordantSpendKey, Proof> {
    /// Generate the proof and the two public keys: the accordant public spend key and the
    /// arbitrating public key, also called the encryption public key,
    fn generate_proof(&mut self) -> Result<(AccordantSpendKey, EncryptionKey, Proof), Error>;

    /// Project the accordant spend secret key over the arbitrating curve to get the public key
    /// used as the encryption public key.
    fn get_encryption_key(&mut self) -> Result<EncryptionKey, Error>;

    /// Verify the proof given the two public keys: the accordant spend public key and the
    /// arbitrating encryption public key.
    fn verify_proof(
        &mut self,
        public_spend: &AccordantSpendKey,
        encryption_key: &EncryptionKey,
        proof: Proof,
    ) -> Result<(), Error>;
}
