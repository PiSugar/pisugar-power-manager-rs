//! OTA file validation and transport are separated so refusal paths can be tested
//! without a Raspberry Pi or access to a target's Flash.
pub mod image;
pub mod protocol;
