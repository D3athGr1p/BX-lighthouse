use beacon_chain::test_utils::{BeaconChainHarness, EphemeralHarnessType};
use beacon_chain::BeaconChain;
use state_processing::state_advance::complete_state_advance;
use std::sync::Arc;
use types::{EthSpec, MainnetEthSpec};
use state_processing::common::increase_balance;

// Test harness for custom reward structure
fn main() {
    env_logger::init();
    
    println!("Testing custom reward structure (10 ETH per slot for first 3 epochs)");
    
    // Create a testing harness
    let harness = BeaconChainHarness::builder(MainnetEthSpec)
        .default_spec()
        .deterministic_keypairs(64)
        .fresh_ephemeral_store()
        .build();
    
    let spec = harness.spec.clone();
    
    // Initialize the beacon chain with genesis
    let chain = harness.chain;
    
    // Get the genesis state
    let genesis_state = chain.canonical_head.cached_head().snapshot.beacon_state.clone();
    println!("Genesis state slot: {}", genesis_state.slot());
    
    // Verify initial balances
    println!("Initial proposer balance: {}", 
             chain.canonical_head.cached_head().snapshot.beacon_state.balances().to_vec()[0]);
    
    // Helper function to advance the chain by a certain number of slots
    // Since we can't directly modify state in the test, we'll document expected rewards
    fn advance_chain_by_slots<E: EthSpec>(
        chain: &Arc<BeaconChain<EphemeralHarnessType<E>>>,
        slots: u64,
    ) -> Result<(), String> {
        let head = chain.canonical_head.cached_head();
        let mut state = head.snapshot.beacon_state.clone();
        
        // Track proposers who should get rewards
        let mut expected_rewards = Vec::new();
        
        // Process each slot
        for _ in 0..slots {
            let next_slot = state.slot() + 1;
            let current_epoch = next_slot / E::slots_per_epoch();
            
            // Get proposer before advancing state
            let proposer_index = state
                .get_beacon_proposer_index(next_slot, &chain.spec)
                .unwrap_or(0);
            
            // Advance the state to the next slot
            complete_state_advance(&mut state, None, next_slot, &chain.spec)
                .map_err(|e| format!("Error advancing state: {:?}", e))?;
            
            // In the first three epochs, proposers should get 10 ETH rewards
            if current_epoch < 3 {
                expected_rewards.push(proposer_index);
                println!("Proposer {} at slot {} should receive 10 ETH reward", proposer_index, next_slot);
            }
            
            // Record the proposer at this slot
            println!("Slot: {}, Proposer: {}", next_slot, proposer_index);
        }
        
        // Test simulation - modify rewards file to check values
        println!("Expected {} proposers to receive rewards: {:?}", expected_rewards.len(), expected_rewards);
        
        Ok(())
    }
    
    // Test rewards structure for multiple epochs
    fn test_rewards_structure<E: EthSpec>(
        chain: &Arc<BeaconChain<EphemeralHarnessType<E>>>,
        epochs_to_test: u64,
    ) {
        println!("\nTesting rewards across {} epochs", epochs_to_test);
        
        let slots_per_epoch = E::slots_per_epoch();
        
        for epoch in 0..epochs_to_test {
            println!("\n=== Testing Epoch {} ===", epoch);
            
            // Store initial balances
            let initial_state = chain.canonical_head.cached_head().snapshot.beacon_state.clone();
            let initial_balances = initial_state.balances().clone();
            
            // Advance by one epoch
            if let Err(e) = advance_chain_by_slots(chain, slots_per_epoch) {
                println!("{}", e);
                return;
            }
            
            // Get new balances
            let new_state = chain.canonical_head.cached_head().snapshot.beacon_state.clone();
            let new_balances = new_state.balances();
            
            // Calculate and display balance changes for each validator
            let initial_balances_vec = initial_balances.to_vec();
            let new_balances_vec = new_balances.to_vec();
            
            let mut reward_count = 0;
            let mut total_rewards = 0;
            
            println!("\nBalance changes for Epoch {}:", epoch);
            
            for (i, (old, new)) in initial_balances_vec.iter().zip(new_balances_vec.iter()).enumerate() {
                let diff = if new > old {
                    new - old
                } else {
                    0
                };
                
                // Always show the balance of the first few validators to debug
                if i < 5 || diff > 0 {
                    println!("Validator {}: {} -> {} (diff: +{} Gwei)", 
                        i, old, new, diff);
                }
                
                if diff > 0 {
                    reward_count += 1;
                    total_rewards += diff;
                    
                    // Verify the reward amount based on our custom structure
                    if epoch < 3 {
                        // Check if the validator was a proposer and got exactly 10 ETH
                        if diff == 10_000_000_000 {
                            println!("✅ Validator {} received correct proposer reward: 10 ETH", i);
                        } else {
                            println!("❌ Validator {} received incorrect reward: {} Gwei", i, diff);
                        }
                    } else {
                        // No rewards should be given after epoch 2
                        println!("❌ Validator {} received reward after epoch limit: {} Gwei", i, diff);
                    }
                }
            }
            
            // Summary for this epoch
            println!("\nEpoch {} summary: {} validators rewarded, total rewards: {} Gwei", 
                epoch, reward_count, total_rewards);
            
            if epoch < 3 {
                if reward_count == 0 {
                    println!("❌ ERROR: No rewards distributed in epoch {}, expected rewards for proposers", epoch);
                } else {
                    let expected_reward: u64 = 10_000_000_000; // 10 ETH in Gwei
                    println!("Expected reward per proposer: {} Gwei", expected_reward);
                }
            } else {
                if reward_count > 0 {
                    println!("❌ ERROR: {} validators rewarded in epoch {}, expected no rewards after epoch 2", 
                        reward_count, epoch);
                } else {
                    println!("✅ No rewards distributed after epoch 2 as expected");
                }
            }
        }
    }
    
    // Test for 5 epochs to verify our 3 epoch cutoff
    test_rewards_structure::<MainnetEthSpec>(&chain, 5);
    
    println!("\nTest completed!");
}
