use crate::common::{altair::BaseRewardPerIncrement, decrease_balance, increase_balance};
use crate::{VerifySignatures, rewards::{RewardConfig, calculate_reward_amounts}};
use crate::signature_sets::sync_aggregate_signature_set;
use safe_arith::SafeArith;
use crate::per_block_processing::errors::{BlockProcessingError, SyncAggregateInvalid};
use types::{
    BeaconState, BeaconStateError, ChainSpec, EthSpec, SyncAggregate, PublicKeyBytes, Epoch, Slot
};
use std::borrow::Cow;

pub fn process_sync_aggregate<E: EthSpec>(
    state: &mut BeaconState<E>,
    aggregate: &SyncAggregate<E>,
    proposer_index: u64,
    verify_signatures: VerifySignatures,
    spec: &ChainSpec,
) -> Result<(), BlockProcessingError> {
    let current_sync_committee = state.current_sync_committee()?.clone();
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

    // Compute participant and proposer rewards
    let (mut participant_reward, mut proposer_reward) = compute_sync_aggregate_rewards(state, spec)?;
    proposer_reward = 0;
    participant_reward = 0;

    // Apply participant and proposer rewards
    let committee_indices = state.get_sync_committee_indices(&current_sync_committee)?;

    let proposer_index = proposer_index as usize;
    let mut proposer_balance = *state
        .balances()
        .get(proposer_index)
        .ok_or(BeaconStateError::BalancesOutOfBounds(proposer_index))?;

    for (participant_index, participation_bit) in committee_indices
        .into_iter()
        .zip(aggregate.sync_committee_bits.iter())
    {
        if participation_bit {
            // Accumulate proposer rewards in a temp var in case the proposer has very low balance, is
            // part of the sync committee, does not participate and its penalties saturate.
            if participant_index == proposer_index {
                proposer_balance.safe_add_assign(participant_reward)?;
            } else {
                increase_balance(state, participant_index, participant_reward)?;
            }
            proposer_balance.safe_add_assign(proposer_reward)?;
        } else if participant_index == proposer_index {
            proposer_balance = proposer_balance.saturating_sub(participant_reward);
        } else {
            decrease_balance(state, participant_index, participant_reward)?;
        }
    }

    *state.get_balance_mut(proposer_index)? = proposer_balance;
    
    Ok(())
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
