//! The electrical view of the shared parts catalogue — now literally the
//! shared `roboparts` schema (review finding resolved: no more twin structs
//! over the same files).

pub use roboparts::{sha256_hex, BusDecl, Catalogue, ChargeProfile, Elec, Part, PinDecl, SourceDecl};

/// Legacy aliases (robowire grew up calling these Elec*).
pub type ElecPart = roboparts::Part;
pub type ElecCatalogue = roboparts::Catalogue;
