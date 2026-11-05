//! Tests that need a process of their own (T0/T1): `Language::inject` swaps the translations of the whole process,
//! which the golden tests running beside it would see.

mod language_inject;
