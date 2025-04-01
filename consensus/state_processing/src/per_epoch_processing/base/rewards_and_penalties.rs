use crate::common::{
    base::{get_base_reward, SqrtTotalActiveBalance},
    decrease_balance, increase_balance,
};
use crate::per_epoch_processing::{
    base::{TotalBalances, ValidatorStatus, ValidatorStatuses},
    Delta, Error,
};
use safe_arith::SafeArith;
use types::{BeaconState, ChainSpec, EthSpec, Slot};

/// Combination of several deltas for different components of an attestation reward.
///
/// Exists only for compatibility with EF rewards tests.
#[derive(Default, Clone)]
pub struct AttestationDelta {
    pub source_delta: Delta,
    pub target_delta: Delta,
    pub head_delta: Delta,
    pub inclusion_delay_delta: Delta,
    pub inactivity_penalty_delta: Delta,
}

impl AttestationDelta {
    /// Flatten into a single delta.
    pub fn flatten(self) -> Result<Delta, Error> {
        let AttestationDelta {
            source_delta,
            target_delta,
            head_delta,
            inclusion_delay_delta,
            inactivity_penalty_delta,
        } = self;
        let mut result = Delta::default();
        for delta in [
            source_delta,
            target_delta,
            head_delta,
            inclusion_delay_delta,
            inactivity_penalty_delta,
        ] {
            result.combine(delta)?;
        }
        Ok(result)
    }
}

#[derive(Debug)]
pub enum ProposerRewardCalculation {
    Include,
    Exclude,
}

/// Apply proposer rewards based on custom fixed reward structure.
/// 
/// - Only first 3 epochs receive rewards (epoch 0, 1, 2)
/// - Each slot's proposer receives exactly 10 ETH
/// - No rewards for attesters or other participants
pub fn process_rewards_and_penalties<E: EthSpec>(
    state: &mut BeaconState<E>,
    _validator_statuses: &ValidatorStatuses,
    spec: &ChainSpec,
) -> Result<(), Error> {
    let current_epoch = state.current_epoch();
    
    // Only apply rewards within first 3 epochs
    if current_epoch.as_u64() > 2 {
        return Ok(());
    }

    // For each slot in the current epoch, reward the proposer directly
    let slots_per_epoch = E::slots_per_epoch();
    let epoch_start_slot = current_epoch.start_slot(E::slots_per_epoch());
    
    // Debug output
    println!("Applying rewards for epoch {}, slots {} to {}", 
             current_epoch, 
             epoch_start_slot, 
             epoch_start_slot + slots_per_epoch - 1);
    
    // For each slot in the current epoch, find and reward the proposer
    for i in 0..slots_per_epoch {
        let slot = epoch_start_slot + i;
        
        // Find the proposer for this slot
        match state.get_beacon_proposer_index(slot, spec) {
            Ok(proposer_index) => {
                // Fixed reward: 10 ETH (10,000,000,000 Gwei)
                let old_balance = state.get_balance(proposer_index).unwrap_or(0);
                increase_balance(state, proposer_index, 10_000_000_000)?;
                let new_balance = state.get_balance(proposer_index).unwrap_or(0);
                
                println!("Rewarded proposer {} for slot {}: {} -> {} (+10 ETH)", 
                         proposer_index, slot, old_balance, new_balance);
            },
            Err(e) => {
                println!("Could not find proposer for slot {}: {:?}", slot, e);
            }
        }
    }
    
    Ok(())
}

/// Apply rewards for participation in attestations during the previous epoch.
pub fn get_attestation_deltas_all<E: EthSpec>(
    state: &BeaconState<E>,
    validator_statuses: &ValidatorStatuses,
    proposer_reward: ProposerRewardCalculation,
    spec: &ChainSpec,
) -> Result<Vec<AttestationDelta>, Error> {
    get_attestation_deltas(state, validator_statuses, proposer_reward, None, spec)
}

/// Apply rewards for participation in attestations during the previous epoch, and only compute
/// rewards for a subset of validators.
pub fn get_attestation_deltas_subset<E: EthSpec>(
    state: &BeaconState<E>,
    validator_statuses: &ValidatorStatuses,
    proposer_reward: ProposerRewardCalculation,
    validators_subset: &Vec<usize>,
    spec: &ChainSpec,
) -> Result<Vec<(usize, AttestationDelta)>, Error> {
    get_attestation_deltas(
        state,
        validator_statuses,
        proposer_reward,
        Some(validators_subset),
        spec,
    )
    .map(|deltas| {
        deltas
            .into_iter()
            .enumerate()
            .filter(|(index, _)| validators_subset.contains(index))
            .collect()
    })
}

/// Apply rewards for participation in attestations during the previous epoch.
/// If `maybe_validators_subset` specified, only the deltas for the specified validator subset is
/// returned, otherwise deltas for all validators are returned.
///
/// Returns a vec of validator indices to `AttestationDelta`.
fn get_attestation_deltas<E: EthSpec>(
    state: &BeaconState<E>,
    validator_statuses: &ValidatorStatuses,
    proposer_reward: ProposerRewardCalculation,
    maybe_validators_subset: Option<&Vec<usize>>,
    spec: &ChainSpec,
) -> Result<Vec<AttestationDelta>, Error> {
    let finality_delay = state
        .previous_epoch()
        .safe_sub(state.finalized_checkpoint().epoch)?
        .as_u64();

    let mut deltas = vec![AttestationDelta::default(); state.validators().len()];

    let total_balances = &validator_statuses.total_balances;
    let sqrt_total_active_balance = SqrtTotalActiveBalance::new(total_balances.current_epoch());

    // Ignore validator if a subset is specified and validator is not in the subset
    let include_validator_delta = |idx| match maybe_validators_subset.as_ref() {
        None => true,
        Some(validators_subset) if validators_subset.contains(&idx) => true,
        Some(_) => false,
    };

    for (index, validator) in validator_statuses.statuses.iter().enumerate() {
        // Ignore ineligible validators. All sub-functions of the spec do this except for
        // `get_inclusion_delay_deltas`. It's safe to do so here because any validator that is in
        // the unslashed indices of the matching source attestations is active, and therefore
        // eligible.
        if !validator.is_eligible {
            continue;
        }

        let base_reward = get_base_reward(
            validator.current_epoch_effective_balance,
            sqrt_total_active_balance,
            spec,
        )?;

        let (inclusion_delay_delta, proposer_delta) =
            get_inclusion_delay_delta(validator, base_reward, spec)?;

        if include_validator_delta(index) {
            let source_delta =
                get_source_delta(validator, base_reward, total_balances, finality_delay, spec)?;
            let target_delta =
                get_target_delta(validator, base_reward, total_balances, finality_delay, spec)?;
            let head_delta =
                get_head_delta(validator, base_reward, total_balances, finality_delay, spec)?;
            let inactivity_penalty_delta =
                get_inactivity_penalty_delta(validator, base_reward, finality_delay, spec)?;

            let delta = deltas
                .get_mut(index)
                .ok_or(Error::DeltaOutOfBounds(index))?;
            delta.source_delta.combine(source_delta)?;
            delta.target_delta.combine(target_delta)?;
            delta.head_delta.combine(head_delta)?;
            delta.inclusion_delay_delta.combine(inclusion_delay_delta)?;
            delta
                .inactivity_penalty_delta
                .combine(inactivity_penalty_delta)?;
        }

        if let ProposerRewardCalculation::Include = proposer_reward {
            if let Some((proposer_index, proposer_delta)) = proposer_delta {
                if include_validator_delta(proposer_index) {
                    deltas
                        .get_mut(proposer_index)
                        .ok_or(Error::ValidatorStatusesInconsistent)?
                        .inclusion_delay_delta
                        .combine(proposer_delta)?;
                }
            }
        }
    }

    Ok(deltas)
}

pub fn get_attestation_component_delta(
    index_in_unslashed_attesting_indices: bool,
    attesting_balance: u64,
    total_balances: &TotalBalances,
    base_reward: u64,
    finality_delay: u64,
    spec: &ChainSpec,
) -> Result<Delta, Error> {
    let mut delta = Delta::default();

    let total_balance = total_balances.current_epoch();

    if index_in_unslashed_attesting_indices {
        if finality_delay > spec.min_epochs_to_inactivity_penalty {
            // Since full base reward will be canceled out by inactivity penalty deltas,
            // optimal participation receives full base reward compensation here.
            delta.reward(base_reward)?;
        } else {
            let reward_numerator = base_reward
                .safe_mul(attesting_balance.safe_div(spec.effective_balance_increment)?)?;
            delta.reward(
                reward_numerator
                    .safe_div(total_balance.safe_div(spec.effective_balance_increment)?)?,
            )?;
        }
    } else {
        delta.penalize(base_reward)?;
    }

    Ok(delta)
}

fn get_source_delta(
    validator: &ValidatorStatus,
    base_reward: u64,
    total_balances: &TotalBalances,
    finality_delay: u64,
    spec: &ChainSpec,
) -> Result<Delta, Error> {
    get_attestation_component_delta(
        validator.is_previous_epoch_attester && !validator.is_slashed,
        total_balances.previous_epoch_attesters(),
        total_balances,
        base_reward,
        finality_delay,
        spec,
    )
}

fn get_target_delta(
    validator: &ValidatorStatus,
    base_reward: u64,
    total_balances: &TotalBalances,
    finality_delay: u64,
    spec: &ChainSpec,
) -> Result<Delta, Error> {
    get_attestation_component_delta(
        validator.is_previous_epoch_target_attester && !validator.is_slashed,
        total_balances.previous_epoch_target_attesters(),
        total_balances,
        base_reward,
        finality_delay,
        spec,
    )
}

fn get_head_delta(
    validator: &ValidatorStatus,
    base_reward: u64,
    total_balances: &TotalBalances,
    finality_delay: u64,
    spec: &ChainSpec,
) -> Result<Delta, Error> {
    get_attestation_component_delta(
        validator.is_previous_epoch_head_attester && !validator.is_slashed,
        total_balances.previous_epoch_head_attesters(),
        total_balances,
        base_reward,
        finality_delay,
        spec,
    )
}

pub fn get_inclusion_delay_delta(
    validator: &ValidatorStatus,
    base_reward: u64,
    spec: &ChainSpec,
) -> Result<(Delta, Option<(usize, Delta)>), Error> {
    // Spec: `index in get_unslashed_attesting_indices(state, matching_source_attestations)`
    if validator.is_previous_epoch_attester && !validator.is_slashed {
        let mut delta = Delta::default();
        let mut proposer_delta = Delta::default();

        let inclusion_info = validator
            .inclusion_info
            .ok_or(Error::ValidatorStatusesInconsistent)?;

        let proposer_reward = get_proposer_reward(base_reward, spec)?;
        proposer_delta.reward(proposer_reward)?;
        let max_attester_reward = base_reward.safe_sub(proposer_reward)?;
        delta.reward(max_attester_reward.safe_div(inclusion_info.delay)?)?;

        let proposer_index = inclusion_info.proposer_index;
        Ok((delta, Some((proposer_index, proposer_delta))))
    } else {
        Ok((Delta::default(), None))
    }
}

pub fn get_inactivity_penalty_delta(
    validator: &ValidatorStatus,
    base_reward: u64,
    finality_delay: u64,
    spec: &ChainSpec,
) -> Result<Delta, Error> {
    let mut delta = Delta::default();

    // Inactivity penalty
    if finality_delay > spec.min_epochs_to_inactivity_penalty {
        // If validator is performing optimally this cancels all rewards for a neutral balance
        delta.penalize(
            spec.base_rewards_per_epoch
                .safe_mul(base_reward)?
                .safe_sub(get_proposer_reward(base_reward, spec)?)?,
        )?;

        // Additionally, all validators whose FFG target didn't match are penalized extra
        // This condition is equivalent to this condition from the spec:
        // `index not in get_unslashed_attesting_indices(state, matching_target_attestations)`
        if validator.is_slashed || !validator.is_previous_epoch_target_attester {
            delta.penalize(
                validator
                    .current_epoch_effective_balance
                    .safe_mul(finality_delay)?
                    .safe_div(spec.inactivity_penalty_quotient)?,
            )?;
        }
    }

    Ok(delta)
}

/// Compute the reward awarded to a proposer.
///
/// Returns a fixed amount (0) for our custom reward structure.
pub fn get_proposer_reward(
    _base_reward: u64,
    _spec: &ChainSpec,
) -> Result<u64, Error> {
    // For our custom reward structure, we don't give any proposer rewards here
    // All rewards are managed centrally in per_block_processing.rs
    Ok(0)
}
