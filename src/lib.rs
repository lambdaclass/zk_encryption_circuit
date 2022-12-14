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
use ark_bls12_377::Fr;
use ark_r1cs_std::prelude::{AllocVar, EqGadget};
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
    ciphertext: &[u8],
    proving_key: ProvingKey,
) -> Result<MarlinProof> {
    let rng = &mut simpleworks::marlin::generate_rand();
    let constraint_system = ConstraintSystem::<ConstraintF>::new_ref();

    // TODO: These three blocks of code could be replaced with calls to `new_witness_vec` and
    // `new_input_vec`, but for some reason that makes integration tests break??
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

    let mut ciphertext_circuit: Vec<UInt8Gadget> = Vec::with_capacity(ciphertext.len());
    for byte in ciphertext {
        ciphertext_circuit.push(UInt8Gadget::new_input(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    encrypt_and_generate_constraints(
        &message_circuit,
        &secret_key_circuit,
        &ciphertext_circuit,
        constraint_system.clone(),
    )?;

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

    Ok(proof)
}

pub fn verify_encryption(
    verifying_key: VerifyingKey,
    proof: &MarlinProof,
    ciphertext: &[u8],
) -> Result<bool> {
    let mut ciphertext_as_field_array = vec![];

    for byte in ciphertext {
        let field_array = byte_to_field_array(*byte);
        for field_element in field_array {
            ciphertext_as_field_array.push(field_element);
        }
    }

    simpleworks::marlin::verify_proof(
        verifying_key,
        &ciphertext_as_field_array,
        proof,
        &mut simpleworks::marlin::generate_rand(),
    )
}

fn byte_to_field_array(byte: u8) -> Vec<ConstraintF> {
    let mut ret = vec![];

    for i in 0_i32..8_i32 {
        let bit = (byte & (1 << i)) != 0;
        ret.push(Fr::from(bit));
    }

    ret
}

pub fn synthesize_keys(plaintext_length: usize) -> Result<(ProvingKey, VerifyingKey)> {
    let rng = &mut simpleworks::marlin::generate_rand();
    let universal_srs =
        simpleworks::marlin::generate_universal_srs(1_000_000, 250_000, 3_000_000, rng)?;
    let constraint_system = ConstraintSystem::<ConstraintF>::new_ref();

    let default_message_input = vec![0_u8; plaintext_length];
    let default_secret_key_input = [0_u8; 16];
    let default_ciphertext_input = vec![0_u8; plaintext_length];

    // TODO: These three blocks of code could be replaced with calls to `new_witness_vec` and
    // `new_input_vec`, but for some reason that makes integration tests break??
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

    let mut ciphertext_circuit: Vec<UInt8Gadget> =
        Vec::with_capacity(default_ciphertext_input.len());
    for byte in default_ciphertext_input {
        ciphertext_circuit.push(UInt8Gadget::new_input(constraint_system.clone(), || {
            Ok(byte)
        })?);
    }

    let _ciphertext = encrypt_and_generate_constraints(
        &message_circuit,
        &secret_key_circuit,
        &ciphertext_circuit,
        constraint_system.clone(),
    );

    simpleworks::marlin::generate_proving_and_verifying_keys(&universal_srs, constraint_system)
}

fn encrypt_and_generate_constraints(
    message: &[UInt8Gadget],
    secret_key: &[UInt8Gadget],
    ciphertext: &[UInt8Gadget],
    cs: ConstraintSystemRef<ConstraintF>,
) -> Result<()> {
    let mut computed_ciphertext: Vec<UInt8Gadget> = Vec::new();
    let lookup_table = aes_circuit::lookup_table(cs.clone())?;
    let round_keys = aes_circuit::derive_keys(secret_key, &lookup_table, cs.clone())?;

    for block in message.chunks(16) {
        // Step 0
        let mut after_add_round_key = aes_circuit::add_round_key(block, secret_key)?;
        // Starting at 1 will skip the first round key which is the same as
        // the secret key.
        for round in 1_usize..=10_usize {
            // Step 1
            let after_substitute_bytes =
                aes_circuit::substitute_bytes(&after_add_round_key, &lookup_table)?;
            // Step 2
            let after_shift_rows = aes_circuit::shift_rows(&after_substitute_bytes, cs.clone())
                .to_anyhow("Error shifting rows")?;
            // Step 3
            // TODO: This mix columns operation is being done on the last round, but it's not taken into
            // account. To increase performance we could move this inside the if statement below.
            let after_mix_columns = aes_circuit::mix_columns(&after_shift_rows, cs.clone())
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
            ciphertext_chunk.push(u8_gadget);
        }

        computed_ciphertext.extend_from_slice(&ciphertext_chunk);
    }

    for (i, byte) in ciphertext.iter().enumerate() {
        byte.enforce_equal(
            ciphertext
                .get(i)
                .to_anyhow("Error getting ciphertext byte")?,
        )?;
    }

    Ok(())
}
