//! Parts catalogue — re-exported from the shared `roboparts` crate (one
//! Part schema, one hash rule, N consumers; review finding resolved).

pub use roboparts::{
    BusDecl, Catalogue, Elec, MotorProps, Part, PinDecl, SourceDecl, TyreProps,
};

/// Kept for source compatibility with earlier partial-mirror names.
pub use roboparts::Elec as ElecInfo;
pub use roboparts::SourceDecl as SourceInfo;
