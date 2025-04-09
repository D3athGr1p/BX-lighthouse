use types::{BeaconState, EthSpec, Epoch, SyncAggregate, Slot};
use std::collections::HashSet;

/// Central reward configuration for the blockchain system
pub struct RewardConfig {
    /// Reward amount for block proposers (in Gwei) during the initial epochs
    pub proposer_reward_initial: u64,
    /// Reward amount for attestations (in Gwei) during the initial epochs
    pub attestation_reward_initial: u64,
    /// Reward amount for sync committee (in Gwei) during the initial epochs
    pub sync_committee_reward_initial: u64,
    /// Reward amount for attestations (in Gwei) after the initial epochs
    pub attestation_reward_ongoing: u64,
    /// Reward amount for sync committee (in Gwei) after the initial epochs
    pub sync_committee_reward_ongoing: u64,
    /// Number of epochs for initial rewards period
    pub initial_reward_epochs: u64,
}

// impl Default for RewardConfig {
//     fn default() -> Self {
//         Self {
//             // Scale the rewards appropriately for 1024 ETH validator balance
//             // Initial rewards (first few epochs) - higher to incentivize participation
//             proposer_reward_initial: 1_000_000_000, // 1 ETH in Gwei
//             attestation_reward_initial: 500_000_000, // 0.5 ETH in Gwei
//             sync_committee_reward_initial: 300_000_000, // 0.3 ETH in Gwei
            
//             // Ongoing rewards after initial period
//             attestation_reward_ongoing: 200_000_000, // 0.2 ETH in Gwei
//             sync_committee_reward_ongoing: 100_000_000, // 0.1 ETH in Gwei
            
//             // Initial reward period lasts for 10 epochs
//             initial_reward_epochs: 10,
//         }
//     }
// }

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            // Scale the rewards appropriately for 1024 ETH validator balance
            // Initial rewards (first few epochs) - higher to incentivize participation
            proposer_reward_initial: 10_000_000_000, // 10 ETH in Gwei
            attestation_reward_initial: 1_00_000, // 0.0001 ETH in Gwei
            sync_committee_reward_initial: 1_00_000, // 0.0001 ETH in Gwei
            
            // Ongoing rewards after initial period
            attestation_reward_ongoing: 1_00_000, // 0.2 ETH in Gwei
            sync_committee_reward_ongoing: 1_00_000, // 0.1 ETH in Gwei
            
            // Initial reward period lasts for 10 epochs
            initial_reward_epochs: 10,
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
pub fn calculate_reward_amounts(
    current_epoch: Epoch,
    config: &RewardConfig,
) -> RewardAmounts {
    let in_initial_period = current_epoch.as_u64() <= config.initial_reward_epochs;
    
    if in_initial_period {
        RewardAmounts {
            proposer_reward: config.proposer_reward_initial,
            attestation_reward: config.attestation_reward_initial,
            sync_committee_reward: config.sync_committee_reward_initial,
        }
    } else {
        RewardAmounts {
            proposer_reward: 0, // 5 ETH for ongoing proposer rewards
            attestation_reward: config.attestation_reward_ongoing,
            sync_committee_reward: config.sync_committee_reward_ongoing,
        }
    }
}

/// Apply the proposer reward to the given validator
pub fn apply_proposer_reward<E: EthSpec>(
    state: &mut BeaconState<E>,
    proposer_index: u64,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

    if let Ok(balance) = state.get_balance_mut(proposer_index as usize) {
        *balance = balance.saturating_add(reward_amount);
        Ok(())
    } else {
        Err("Failed to get proposer balance")
    }
}

/// Collect all validator indices that are eligible for attestation rewards
pub fn collect_attesting_validators<E: EthSpec>(
    state: &BeaconState<E>,
) -> Vec<usize> {
    let mut validators_to_reward = HashSet::new();
    
    // Previous epoch attesters
    if let Ok(previous_epoch_participation) = state.previous_epoch_participation() {
        for (validator_index, participation) in previous_epoch_participation.iter().enumerate() {
            // Check if any participation flag is set
            if participation.into_u8() > 0 {
                validators_to_reward.insert(validator_index);
                // println!("Validator {} had participation flags set in previous epoch", validator_index);
            }
        }
    }
    
    // Current epoch attesters
    if let Ok(current_epoch_participation) = state.current_epoch_participation() {
        for (validator_index, participation) in current_epoch_participation.iter().enumerate() {
            // Check if any participation flag is set
            if participation.into_u8() > 0 {
                validators_to_reward.insert(validator_index);
                // println!("Validator {} had participation flags set in current epoch", validator_index);
            }
        }
    }
    
    // Fallback: If no validators found with participation flags, include all active validators
    // This ensures rewards continue even if participation tracking has issues
    if validators_to_reward.is_empty() {
        println!("WARNING: No validators found with participation flags. Adding all active validators.");
        for (validator_index, validator) in state.validators().iter().enumerate() {
            if validator.is_active_at(state.current_epoch()) {
                validators_to_reward.insert(validator_index);
            }
        }
    }
    
    let result: Vec<usize> = validators_to_reward.into_iter().collect();
    println!("Found {} validators to reward for attestations", result.len());
    result
}

/// Apply attestation rewards to all eligible validators
pub fn apply_attestation_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

    // Collect validators that need rewards
    let validators_to_reward = collect_attesting_validators(state);
    
    // Apply rewards
    for validator_index in validators_to_reward {
        if let Ok(balance) = state.get_balance_mut(validator_index) {
            *balance = balance.saturating_add(reward_amount);
            // println!("Applied {} Gwei attestation reward to validator {}", reward_amount, validator_index);
        }
    }
    
    Ok(())
}

/// Apply sync committee rewards based on sync aggregate
pub fn apply_sync_committee_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    sync_aggregate: &SyncAggregate<E>,
    reward_amount: u64,
) -> Result<(), &'static str> {
    if reward_amount == 0 {
        return Ok(());
    }

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
    
    // Apply rewards to the correct validators who participated
    for validator_index in sync_committee_indices {
        if let Ok(balance) = state.get_balance_mut(validator_index) {
            *balance = balance.saturating_add(reward_amount);
            // println!("Rewarded sync committee validator {} with {} Gwei", validator_index, reward_amount);
        }
    }
    
    Ok(())
}

/// Apply all rewards in one consolidated function
pub fn apply_all_rewards<E: EthSpec>(
    state: &mut BeaconState<E>,
    proposer_index: u64,
    sync_aggregate_opt: Option<&SyncAggregate<E>>,
    current_epoch: Epoch,
    slot: Slot,
    config: &RewardConfig,
) -> Result<(), &'static str> {
    // Calculate reward amounts for the current epoch
    let reward_amounts = calculate_reward_amounts(current_epoch, config);
    
    // Apply proposer reward
    if let Err(e) = apply_proposer_reward(state, proposer_index, reward_amounts.proposer_reward) {
        println!("Warning: Failed to apply proposer reward: {}", e);
    } else if reward_amounts.proposer_reward > 0 {
        println!(
            "Applied {} Gwei reward to proposer {} in epoch {} slot {}", 
            reward_amounts.proposer_reward, proposer_index, current_epoch, slot
        );
    }
    
    // Apply attestation rewards
    if let Err(e) = apply_attestation_rewards(state, reward_amounts.attestation_reward) {
        println!("Warning: Failed to apply attestation rewards: {}", e);
    }
    
    // Apply sync committee rewards if aggregate is available
    if let Some(sync_aggregate) = sync_aggregate_opt {
        if let Err(e) = apply_sync_committee_rewards(state, sync_aggregate, reward_amounts.sync_committee_reward) {
            println!("Warning: Failed to apply sync committee rewards: {}", e);
        }
    }
    
    // Log summary
    if current_epoch.as_u64() <= config.initial_reward_epochs {
        println!(
            "Applied rewards in epoch {} slot {}: proposer={}, attestation={}, sync={}", 
            current_epoch, slot, 
            reward_amounts.proposer_reward,
            reward_amounts.attestation_reward,
            reward_amounts.sync_committee_reward
        );
    } else {
        println!(
            "Applied minimal rewards in epoch {} slot {} (beyond initial reward period)", 
            current_epoch, slot
        );
    }
    
    Ok(())
}
