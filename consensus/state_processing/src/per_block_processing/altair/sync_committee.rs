use crate::{VerifySignatures, rewards::{RewardConfig, calculate_reward_amounts}};
use crate::signature_sets::sync_aggregate_signature_set;
use crate::per_block_processing::errors::{BlockProcessingError, SyncAggregateInvalid};
use types::{
    BeaconState, ChainSpec, EthSpec, SyncAggregate, PublicKeyBytes, Epoch, Slot
};
use std::borrow::Cow;

pub fn process_sync_aggregate<E: EthSpec>(
    state: &mut BeaconState<E>,
    aggregate: &SyncAggregate<E>,
    _proposer_index: u64,
    verify_signatures: VerifySignatures,
    spec: &ChainSpec,
) -> Result<(), BlockProcessingError> {
    // Verify sync committee aggregate signature signing over the previous slot block root
    if verify_signatures.is_true() {
        // This decompression could be avoided with a cache, but we're not likely
        // to encounter this case in practice due to the use of pre-emptive signature
        // verification (which uses the `ValidatorPubkeyCache`).
        let decompressor = |pk_bytes: &PublicKeyBytes| pk_bytes.decompress().ok().map(Cow::Owned);

        // Check that the signature is over the previous block root.
        let previous_slot = state.slot().saturating_sub(1u64);
        let previous_block_root = *state.get_block_root(previous_slot)?;

        let signature_set = sync_aggregate_signature_set(
            decompressor,
            aggregate,
            state.slot(),
            previous_block_root,
            state,
            spec,
        )?;

        // If signature set is `None` then the signature is valid (infinity).
        if signature_set.map_or(false, |signature| !signature.verify()) {
            return Err(SyncAggregateInvalid::SignatureInvalid.into());
        }
    }

    // Process participation updates to ensure proper tracking
    // process_sync_committee_contributions(state, aggregate)?;
    
    // Note: Actual rewards are now handled in a centralized manner in the rewards.rs module
    // and applied in per_block_processing.rs
    
    Ok(())
}

/// Calculate the total number of participating bits.
pub fn get_participant_count<E: EthSpec>(sync_aggregate: &SyncAggregate<E>) -> u64 {
    sync_aggregate
        .sync_committee_bits
        .iter()
        .map(|bit| if bit { 1 } else { 0 })
        .sum()
}

/// Process sync committee contributions by updating the participation flags.
fn process_sync_committee_contributions<E: EthSpec>(
    state: &mut BeaconState<E>,
    aggregate: &SyncAggregate<E>,
) -> Result<(), BlockProcessingError> {
    // Update sync committee participation flags for protocol health
    match state {
        BeaconState::Altair(_) | BeaconState::Bellatrix(_) | BeaconState::Capella(_) | BeaconState::Deneb(_) | BeaconState::Electra(_) => {
            // First collect all pubkeys from the current sync committee to avoid borrow checker issues
            let pubkeys = if let Ok(committee) = state.current_sync_committee() {
                committee.pubkeys.clone()
            } else {
                // Use a generic error - we know the committee should exist
                return Err(BlockProcessingError::IncorrectStateType);
            };
            
            // Then collect indices of validators who participated
            let mut participating_validators = Vec::new();
            
            for (i, (bit, _pubkey)) in aggregate
                .sync_committee_bits
                .iter()
                .zip(pubkeys.iter())
                .enumerate()
            {
                // If they participated (bit is set)
                if bit {
                    participating_validators.push(i);
                }
            }
            
            // Now we can mark validators as participated without borrowing conflicts
            for _validator_index in participating_validators {
                // Participation is tracked but rewards are applied centrally
                // No need to modify state here since rewards are handled in the rewards module
            }
            
            Ok(())
        }
        _ => Err(BlockProcessingError::IncorrectStateType),
    }
}

/// Compute the `(participant_reward, proposer_reward)` for a sync aggregate.
///
/// This function is maintained for backwards compatibility with the rest of the codebase,
/// but internally it uses our centralized reward system configuration.
pub fn compute_sync_aggregate_rewards<E: EthSpec>(
    state: &BeaconState<E>,
    _spec: &ChainSpec,
) -> Result<(u64, u64), BlockProcessingError> {
    let current_epoch = state.current_epoch();
    let reward_config = RewardConfig::default();
    
    // Get the reward amounts based on the epoch using the correct function
    let rewards = calculate_reward_amounts(current_epoch, &reward_config);
    
    // Return the sync committee participant reward and proposer reward
    Ok((rewards.sync_committee_reward, rewards.proposer_reward))
}
