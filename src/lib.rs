#![warn(warnings, rust_2018_idioms)]
#![forbid(unsafe_code)]
#![recursion_limit = "256"]
#![warn(
    clippy::allow_attributes_without_reason,
    clippy::as_conversions,
    clippy::as_ptr_cast_mut,
    clippy::unnecessary_cast,
    clippy::clone_on_ref_ptr,
    clippy::create_dir,
    clippy::dbg_macro,
    clippy::decimal_literal_representation,
    clippy::default_numeric_fallback,
    clippy::deref_by_slicing,
    clippy::empty_structs_with_brackets,
    clippy::float_cmp_const,
    clippy::fn_to_numeric_cast_any,
    clippy::indexing_slicing,
    clippy::iter_kv_map,
    clippy::manual_clamp,
    clippy::manual_filter,
    clippy::map_err_ignore,
    clippy::uninlined_format_args,
    clippy::unseparated_literal_suffix,
    clippy::unused_format_specs,
    clippy::single_char_lifetime_names,
    clippy::str_to_string,
    clippy::string_add,
    clippy::string_slice,
    clippy::string_to_string,
    clippy::todo,
    clippy::try_err
)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![allow(
    clippy::module_inception,
    clippy::module_name_repetitions,
    clippy::let_underscore_must_use
)]

pub mod aes;
pub mod aes_circuit;
pub mod helpers;
pub mod ops;

use anyhow::{anyhow, Result};
use ark_r1cs_std::{prelude::AllocVar, R1CSVar};
use ark_relations::r1cs::{ConstraintSystem, ConstraintSystemRef};
use helpers::traits::ToAnyhow;
use simpleworks::gadgets::UInt8Gadget;
pub use simpleworks::marlin::generate_rand;
pub use simpleworks::marlin::serialization::deserialize_proof;
use simpleworks::{
    gadgets::ConstraintF,
    marlin::{MarlinProof, ProvingKey, VerifyingKey},
};
use std::cell::RefCell;
use std::rc::Rc;

pub fn encrypt(
    message: &[u8],
    secret_key: &[u8; 16],
    proving_key: ProvingKey,
) -> Result<(Vec<u8>, MarlinProof)> {
    let rng = &mut simpleworks::marlin::generate_rand();
    let constraint_system = ConstraintSystem::<ConstraintF>::new_ref();

    let mut message_circuit: Vec<UInt8Gadget> = Vec::with_capacity(message.len());
    for byte in message {
        message_circuit.push(UInt8Gadget::new_witness(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    let mut secret_key_circuit: Vec<UInt8Gadget> = Vec::with_capacity(secret_key.len());
    for byte in secret_key {
        secret_key_circuit.push(UInt8Gadget::new_witness(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    let ciphertext = encrypt_and_generate_constraints(&message_circuit, &secret_key_circuit)?;

    // Here we clone the constraint system because deep down when generating
    // the proof the constraint system is consumed and it has to have one
    // reference for it to be consumed.
    let cs_clone = (*constraint_system
        .borrow()
        .ok_or("Error borrowing")
        .map_err(|e| anyhow!("{}", e))?)
    .clone();
    let cs_ref_clone = ConstraintSystemRef::CS(Rc::new(RefCell::new(cs_clone)));

    let proof = simpleworks::marlin::generate_proof(cs_ref_clone, proving_key, rng)?;

    Ok((ciphertext, proof))
}

pub fn verify_encryption(verifying_key: VerifyingKey, proof: &MarlinProof) -> Result<bool> {
    simpleworks::marlin::verify_proof(
        verifying_key,
        &[],
        proof,
        &mut simpleworks::marlin::generate_rand(),
    )
}

pub fn synthesize_keys(plaintex_length: usize) -> Result<(ProvingKey, VerifyingKey)> {
    let rng = &mut simpleworks::marlin::generate_rand();
    let universal_srs =
        simpleworks::marlin::generate_universal_srs(10000000, 2500000, 30000000, rng)?;
    let constraint_system = ConstraintSystem::<ConstraintF>::new_ref();

    let default_message_input = vec![0_u8; plaintex_length];
    let default_secret_key_input = [0_u8; 16];

    let mut message_circuit: Vec<UInt8Gadget> = Vec::with_capacity(default_message_input.len());
    for byte in default_message_input {
        message_circuit.push(UInt8Gadget::new_witness(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    let mut secret_key_circuit: Vec<UInt8Gadget> =
        Vec::with_capacity(default_secret_key_input.len());
    for byte in default_secret_key_input {
        secret_key_circuit.push(UInt8Gadget::new_witness(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    let _ciphertext = encrypt_and_generate_constraints(&message_circuit, &secret_key_circuit);

    simpleworks::marlin::generate_proving_and_verifying_keys(&universal_srs, constraint_system)
}

fn encrypt_and_generate_constraints(
    message: &[UInt8Gadget],
    secret_key: &[UInt8Gadget],
) -> Result<Vec<u8>> {
    let mut ciphertext: Vec<u8> = Vec::new();
    let round_keys = aes_circuit::derive_keys(secret_key)?;

    // TODO: Make this in 10 rounds instead of 1.
    // 1 round ECB
    for block in message.chunks(16) {
        // Step 0
        let mut after_add_round_key = aes_circuit::add_round_key(block, secret_key)?;
        // Starting at 1 will skip the first round key which is the same as
        // the secret key.
        for round in 1_usize..=10_usize {
            // Step 1
            let after_substitute_bytes = aes_circuit::substitute_bytes(&after_add_round_key)?;
            // Step 2
            let after_shift_rows = aes_circuit::shift_rows(&after_substitute_bytes)
                .to_anyhow("Error shifting rows")?;
            // Step 3
            // TODO: This mix columns operation is being done on the last round, but it's not taken into
            // account. To increase performance we could move this inside the if statement below.
            let after_mix_columns = aes_circuit::mix_columns(&after_shift_rows)
                .to_anyhow("Error mixing columns when encrypting")?;
            // Step 4
            // This ciphertext should represent the next round plaintext and use the round key.
            if round < 10_usize {
                after_add_round_key = aes_circuit::add_round_key(
                    &after_mix_columns,
                    round_keys
                        .get(round)
                        .to_anyhow(&format!("Error getting round key in round {round}"))?,
                )?;
            } else {
                after_add_round_key = aes_circuit::add_round_key(
                    &after_shift_rows,
                    round_keys
                        .get(round)
                        .to_anyhow(&format!("Error getting round key in round {round}"))?,
                )?;
            }
        }
        let mut ciphertext_chunk = vec![];

        for u8_gadget in after_add_round_key {
            ciphertext_chunk.push(u8_gadget.value()?);
        }

        ciphertext.extend_from_slice(&ciphertext_chunk);
    }

    Ok(ciphertext)
}
