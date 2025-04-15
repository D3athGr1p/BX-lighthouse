use std::collections::HashSet;
use types::{BeaconState, Epoch, EthSpec, Slot, SyncAggregate};

/// Constants for reward distribution percentages
pub const VALIDATOR_REWARD_PERCENTAGE: u64 = 70;
pub const GRIDBOX_REWARD_PERCENTAGE: u64 = 20;
pub const MARKETING_REWARD_PERCENTAGE: u64 = 10;

/// Fixed indices for special reward addresses
pub const GRIDBOX_ADDRESS_INDEX: usize = 0;
pub const MARKETING_ADDRESS_INDEX: usize = 1;

/// Central reward configuration for the blockchain system
pub struct RewardConfig {
    /// Reward amount for block proposers (in Gwei) during the initial epochs
    pub proposer_reward_initial: u64,
    /// Reward amount for attestations (in Gwei) during the initial epochs
    pub attestation_reward_initial: u64,
    /// Reward amount for sync committee (in Gwei) during the initial epochs
    pub sync_committee_reward_initial: u64,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            // Initial rewards (first few epochs) - higher to incentivize participation
            proposer_reward_initial: 2_600_000_000, // 2.6 ETH in Gwei
            attestation_reward_initial: 1_00_000,   // 0.0001 ETH in Gwei
            sync_committee_reward_initial: 1_00_000, // 0.0001 ETH in Gwei
        }
    }
}

/// Struct containing all current reward amounts based on epoch
pub struct RewardAmounts {
    pub proposer_reward: u64,
    pub attestation_reward: u64,
    pub sync_committee_reward: u64,
}

/// Calculate reward amounts based on the current epoch and reward configuration
pub fn calculate_reward_amounts(current_epoch: Epoch, config: &RewardConfig) -> RewardAmounts {
    let ep = current_epoch.as_u64();
    let mut proposer_reward_amount;

    if ep <= 25200 {
        proposer_reward_amount = 2_600_000_000;
    } else if ep <= 100800 {
        proposer_reward_amount = 2_100_000_000;
    } else if ep <= 176400 {
        proposer_reward_amount = 1_700_000_000;
    } else if ep <= 252000 {
        proposer_reward_amount = 1_300_000_000;
    } else if ep <= 327600 {
        proposer_reward_amount = 1_100_000_000;
    } else if ep <= 403200 {
        proposer_reward_amount = 1_000_000_000;
    } else if ep <= 478800 {
        proposer_reward_amount = 900_000_000;
    } else if ep <= 554400 {
        proposer_reward_amount = 750_000_000;
    } else if ep <= 630000 {
        proposer_reward_amount = 650_000_000;
    } else if ep <= 705600 {
        proposer_reward_amount = 650_000_000;
    } else if ep <= 781200 {
        proposer_reward_amount = 600_000_000;
    } else if ep <= 856800 {
        proposer_reward_amount = 550_000_000;
    } else if ep <= 932400 {
        proposer_reward_amount = 500_000_000;
    } else if ep <= 1008000 {
        proposer_reward_amount = 450_000_000;
    } else if ep <= 1083600 {
        proposer_reward_amount = 400_000_000;
    } else if ep <= 1159200 {
        proposer_reward_amount = 350_000_000;
    } else if ep <= 1234800 {
        proposer_reward_amount = 300_000_000;
    } else if ep <= 1310400 {
        proposer_reward_amount = 250_000_000;
    } else if ep <= 1386000 {
        proposer_reward_amount = 200_000_000;
    } else if ep <= 1461600 {
        proposer_reward_amount = 150_000_000;
    } else if ep <= 1537200 {
        proposer_reward_amount = 100_000_000;
    } else if ep <= 1612800 {
        proposer_reward_amount = 50_000_000;
    } else if ep <= 1688400 {
        proposer_reward_amount = 45_000_000;
    } else if ep <= 1764000 {
        proposer_reward_amount = 40_000_000;
    } else if ep <= 1839600 {
        proposer_reward_amount = 35_000_000;
    } else if ep <= 1915200 {
        proposer_reward_amount = 30_000_000;
    } else if ep <= 1990800 {
        proposer_reward_amount = 25_000_000;
    } else if ep <= 2066400 {
        proposer_reward_amount = 20_000_000;
    } else if ep <= 2142000 {
        proposer_reward_amount = 15_000_000;
    } else if ep <= 2217600 {
        proposer_reward_amount = 10_000_000;
    } else if ep <= 2293200 {
        proposer_reward_amount = 5_000_000;
    } else {
        proposer_reward_amount = 0;
    }

    RewardAmounts {
        proposer_reward: proposer_reward_amount,
        attestation_reward: config.attestation_reward_initial,
        sync_committee_reward: config.sync_committee_reward_initial,
    }
}

/// Apply the proposer reward to the given validator with distribution to dev and charity addresses
pub fn apply_proposer_reward<E: EthSpec>(
    state: &mut BeaconState<E>,
    proposer_index: u64,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

    // Calculate distributed rewards based on percentages
    let validator_reward = reward_amount.saturating_mul(VALIDATOR_REWARD_PERCENTAGE) / 100;
    let dev_reward = reward_amount.saturating_mul(GRIDBOX_REWARD_PERCENTAGE) / 100;
    let charity_reward = reward_amount.saturating_mul(MARKETING_REWARD_PERCENTAGE) / 100;

    // Apply rewards to the proposer validator (70%)
    if let Ok(balance) = state.get_balance_mut(proposer_index as usize) {
        *balance = balance.saturating_add(validator_reward);
    } else {
        return Err("Failed to get proposer balance");
    }

    // Apply dev rewards (20%)
    if let Ok(dev_balance) = state.get_balance_mut(GRIDBOX_ADDRESS_INDEX) {
        *dev_balance = dev_balance.saturating_add(dev_reward);
    } else {
        return Err("Failed to get dev address balance");
    }

    // Apply charity rewards (10%)
    if let Ok(charity_balance) = state.get_balance_mut(MARKETING_ADDRESS_INDEX) {
        *charity_balance = charity_balance.saturating_add(charity_reward);
    } else {
        return Err("Failed to get charity address balance");
    }

    Ok(())
}

/// Collect all validator indices that are eligible for attestation rewards
pub fn collect_attesting_validators<E: EthSpec>(state: &BeaconState<E>) -> Vec<usize> {
    let mut validators_to_reward = HashSet::new();

    // Previous epoch attesters
    if let Ok(previous_epoch_participation) = state.previous_epoch_participation() {
        for (validator_index, participation) in previous_epoch_participation.iter().enumerate() {
            // Check if any participation flag is set
            if participation.into_u8() > 0 {
                validators_to_reward.insert(validator_index);
            }
        }
    }

    // Current epoch attesters
    if let Ok(current_epoch_participation) = state.current_epoch_participation() {
        for (validator_index, participation) in current_epoch_participation.iter().enumerate() {
            // Check if any participation flag is set
            if participation.into_u8() > 0 {
                validators_to_reward.insert(validator_index);
            }
        }
    }

    // Fallback: If no validators found with participation flags, include all active validators
    // This ensures rewards continue even if participation tracking has issues
    if validators_to_reward.is_empty() {
        println!(
            "WARNING: No validators found with participation flags. Adding all active validators."
        );
        for (validator_index, validator) in state.validators().iter().enumerate() {
            if validator.is_active_at(state.current_epoch()) {
                validators_to_reward.insert(validator_index);
            }
        }
    }

    let result: Vec<usize> = validators_to_reward.into_iter().collect();
    result
}

/// Apply attestation rewards to all eligible validators with distribution to dev and charity addresses
pub fn apply_attestation_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

    // Calculate distributed rewards based on percentages
    let validator_reward = reward_amount.saturating_mul(VALIDATOR_REWARD_PERCENTAGE) / 100;
    let dev_reward = reward_amount.saturating_mul(GRIDBOX_REWARD_PERCENTAGE) / 100;
    let charity_reward = reward_amount.saturating_mul(MARKETING_REWARD_PERCENTAGE) / 100;

    // Calculate total dev and charity rewards based on number of validators
    let validators_to_reward = collect_attesting_validators(state);
    let total_dev_reward = dev_reward.saturating_mul(validators_to_reward.len() as u64);
    let total_charity_reward = charity_reward.saturating_mul(validators_to_reward.len() as u64);

    // Apply rewards to individual validators (70%)
    for validator_index in validators_to_reward.iter() {
        if let Ok(balance) = state.get_balance_mut(*validator_index) {
            *balance = balance.saturating_add(validator_reward);
        }
    }

    // Apply dev rewards (20% of total)
    if let Ok(dev_balance) = state.get_balance_mut(GRIDBOX_ADDRESS_INDEX) {
        *dev_balance = dev_balance.saturating_add(total_dev_reward);
    } else {
        return Err("Failed to get dev address balance");
    }

    // Apply charity rewards (10% of total)
    if let Ok(charity_balance) = state.get_balance_mut(MARKETING_ADDRESS_INDEX) {
        *charity_balance = charity_balance.saturating_add(total_charity_reward);
    } else {
        return Err("Failed to get charity address balance");
    }

    Ok(())
}

/// Apply sync committee rewards based on sync aggregate with distribution to dev and charity addresses
pub fn apply_sync_committee_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    sync_aggregate: &SyncAggregate<E>,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

    // Calculate distributed rewards based on percentages
    let validator_reward = reward_amount.saturating_mul(VALIDATOR_REWARD_PERCENTAGE) / 100;
    let dev_reward = reward_amount.saturating_mul(GRIDBOX_REWARD_PERCENTAGE) / 100;
    let charity_reward = reward_amount.saturating_mul(MARKETING_REWARD_PERCENTAGE) / 100;

    // First, collect pubkeys and participation bits without borrowing issues
    let mut sync_committee_pairs = Vec::new();

    if let Ok(committee) = state.current_sync_committee() {
        // Store pubkey and bit position pairs for later processing
        for (i, pubkey) in committee.pubkeys.iter().enumerate() {
            if let Ok(participated) = sync_aggregate.sync_committee_bits.get(i) {
                if participated {
                    // Clone the pubkey to avoid reference issues
                    sync_committee_pairs.push((pubkey.clone(), true));
                }
            }
        }
    }

    // Now find validator indices without borrow conflicts
    let mut sync_committee_indices = Vec::new();
    for (pubkey, _) in sync_committee_pairs.iter() {
        if let Ok(Some(validator_index)) = state.get_validator_index(pubkey) {
            sync_committee_indices.push(validator_index);
        }
    }

    // Calculate total dev and charity rewards based on number of validators
    let total_dev_reward = dev_reward.saturating_mul(sync_committee_indices.len() as u64);
    let total_charity_reward = charity_reward.saturating_mul(sync_committee_indices.len() as u64);

    // Apply rewards to the correct validators who participated (70%)
    for validator_index in sync_committee_indices.iter() {
        if let Ok(balance) = state.get_balance_mut(*validator_index) {
            *balance = balance.saturating_add(validator_reward);
        }
    }

    // Apply dev rewards (20% of total)
    if let Ok(dev_balance) = state.get_balance_mut(GRIDBOX_ADDRESS_INDEX) {
        *dev_balance = dev_balance.saturating_add(total_dev_reward);
    } else {
        return Err("Failed to get dev address balance");
    }

    // Apply charity rewards (10% of total)
    if let Ok(charity_balance) = state.get_balance_mut(MARKETING_ADDRESS_INDEX) {
        *charity_balance = charity_balance.saturating_add(total_charity_reward);
    } else {
        return Err("Failed to get charity address balance");
    }

    Ok(())
}

/// Apply all rewards in one consolidated function
pub fn apply_all_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    proposer_index: u64,
    sync_aggregate_opt: Option<&SyncAggregate<E>>,
    current_epoch: Epoch,
    _slot: Slot,
    config: &RewardConfig,
) -> Result<(), &'static str> {
    // Calculate reward amounts for the current epoch
    let reward_amounts = calculate_reward_amounts(current_epoch, config);

    // Apply proposer reward
    if let Err(e) = apply_proposer_reward(state, proposer_index, reward_amounts.proposer_reward) {
        println!("Warning: Failed to apply proposer reward: {}", e);
    }

    // Apply attestation rewards
    if let Err(e) = apply_attestation_rewards(state, reward_amounts.attestation_reward) {
        println!("Warning: Failed to apply attestation rewards: {}", e);
    }

    // Apply sync committee rewards if aggregate is available
    if let Some(sync_aggregate) = sync_aggregate_opt {
        if let Err(e) = apply_sync_committee_rewards(
            state,
            sync_aggregate,
            reward_amounts.sync_committee_reward,
        ) {
            println!("Warning: Failed to apply sync committee rewards: {}", e);
        }
    }

    Ok(())
}