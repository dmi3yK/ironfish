use std::slice;

use bellman::{
    gadgets::boolean::{self, AllocatedBit, Boolean},
    ConstraintSystem, SynthesisError,
};
use ff::PrimeField;
use zcash_primitives::sapling::ValueCommitment;
use zcash_proofs::{
    circuit::ecc::{self, EdwardsPoint},
    constants::{VALUE_COMMITMENT_RANDOMNESS_GENERATOR, VALUE_COMMITMENT_VALUE_GENERATOR},
};

use crate::constants::proof::ASSET_KEY_GENERATOR;

#[allow(clippy::too_many_arguments)]
pub fn asset_info_preimage<CS: bellman::ConstraintSystem<bls12_381::Scalar>>(
    cs: &mut CS,
    name: &[u8; 32],
    metadata: &[u8; 76],
    asset_public_key: &EdwardsPoint,
    nonce: &u8,
) -> Result<Vec<boolean::Boolean>, SynthesisError> {
    let mut combined_preimage = vec![];

    combined_preimage
        .extend(asset_public_key.repr(cs.namespace(|| "booleanize asset_public_key"))?);

    let name_bits = slice_into_boolean_vec_le(cs.namespace(|| "booleanize name"), Some(name), 32)?;
    combined_preimage.extend(name_bits);

    let metadata_bits =
        slice_into_boolean_vec_le(cs.namespace(|| "booleanize metadata"), Some(metadata), 76)?;
    combined_preimage.extend(metadata_bits);

    let nonce_bits = slice_into_boolean_vec_le(
        cs.namespace(|| "booleanize nonce"),
        Some(slice::from_ref(nonce)),
        1,
    )?;
    combined_preimage.extend(nonce_bits);

    Ok(combined_preimage)
}

pub fn slice_into_boolean_vec_le<Scalar: PrimeField, CS: ConstraintSystem<Scalar>>(
    mut cs: CS,
    value: Option<&[u8]>,
    byte_length: u32,
) -> Result<Vec<Boolean>, SynthesisError> {
    let bit_length = byte_length * 8;
    let values: Vec<Option<bool>> = match value {
        Some(value) => value
            .iter()
            .flat_map(|&v| (0..8).map(move |i| Some((v >> i) & 1 == 1)))
            .collect(),
        None => vec![None; bit_length as usize],
    };

    let bits = values
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            Ok(Boolean::from(AllocatedBit::alloc(
                cs.namespace(|| format!("bit {}", i)),
                b,
            )?))
        })
        .collect::<Result<Vec<_>, SynthesisError>>()?;

    if bits.len() != bit_length as usize {
        // Not the best error type here, but easier than forking the error types right now
        return Err(SynthesisError::Unsatisfiable);
    }

    Ok(bits)
}

/// Exposes a Pedersen commitment to the value as an
/// input to the circuit
pub fn expose_value_commitment<CS>(
    mut cs: CS,
    value_commitment: Option<ValueCommitment>,
) -> Result<Vec<boolean::Boolean>, SynthesisError>
where
    CS: ConstraintSystem<bls12_381::Scalar>,
{
    // Booleanize the value into little-endian bit order
    let value_bits = boolean::u64_into_boolean_vec_le(
        cs.namespace(|| "value"),
        value_commitment.as_ref().map(|c| c.value),
    )?;

    // Compute the note value in the exponent
    let value = ecc::fixed_base_multiplication(
        cs.namespace(|| "compute the value in the exponent"),
        &VALUE_COMMITMENT_VALUE_GENERATOR,
        &value_bits,
    )?;

    // Booleanize the randomness. This does not ensure
    // the bit representation is "in the field" because
    // it doesn't matter for security.
    let rcv = boolean::field_into_boolean_vec_le(
        cs.namespace(|| "rcv"),
        value_commitment.as_ref().map(|c| c.randomness),
    )?;

    // Compute the randomness in the exponent
    let rcv = ecc::fixed_base_multiplication(
        cs.namespace(|| "computation of rcv"),
        &VALUE_COMMITMENT_RANDOMNESS_GENERATOR,
        &rcv,
    )?;

    // Compute the Pedersen commitment to the value
    let cv = value.add(cs.namespace(|| "computation of cv"), &rcv)?;

    // Expose the commitment as an input to the circuit
    cv.inputize(cs.namespace(|| "commitment point"))?;

    Ok(value_bits)
}

pub fn expose_randomized_public_key(
    mut cs: impl ConstraintSystem<bls12_381::Scalar>,
    public_key_randomness: Option<jubjub::Fr>,
    asset_public_key: &EdwardsPoint,
) -> Result<(), SynthesisError> {
    // Randomize public address and expose it
    let public_key_randomness_bits = boolean::field_into_boolean_vec_le(
        cs.namespace(|| "public key randomness"),
        public_key_randomness,
    )?;

    let public_key_randomness = ecc::fixed_base_multiplication(
        cs.namespace(|| "asset public key"),
        &ASSET_KEY_GENERATOR,
        &public_key_randomness_bits,
    )?;

    let randomized_public_key = asset_public_key.add(
        cs.namespace(|| "computation of randomized public key"),
        &public_key_randomness,
    )?;

    randomized_public_key.inputize(cs.namespace(|| "inputize randomized public key"))?;

    Ok(())
}
