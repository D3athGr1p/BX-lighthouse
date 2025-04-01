use crate::common::update_progressive_balances_cache::update_progressive_balances_on_slashing;
use crate::{
    common::decrease_balance,
    per_block_processing::errors::BlockProcessingError,
    ConsensusContext,
};
use safe_arith::SafeArith;
use types::{*};

/// Slash the validator with index `slashed_index`.
pub fn slash_validator<E: EthSpec>(
    state: &mut BeaconState<E>,
    slashed_index: usize,
    opt_whistleblower_index: Option<usize>,
    ctxt: &mut ConsensusContext<E>,
    spec: &ChainSpec,
) -> Result<(), BlockProcessingError> {
    let epoch = state.current_epoch();
    let latest_block_slot = state.latest_block_header().slot;

    // Avoid slashing a validator twice
    let validator = state
        .validators_mut()
        .get_mut(slashed_index)
        .ok_or(Error::UnknownValidator(slashed_index))?;

    if validator.slashed {
        return Err(Error::ValidatorIsWithdrawable.into());
    }

    // Set slashed flag
    validator.slashed = true;
    
    // Update withdrawable epoch
    validator.withdrawable_epoch = std::cmp::max(
        validator.withdrawable_epoch,
        epoch
            .safe_add(E::EpochsPerSlashingsVector::to_u64())?
            .safe_add(Epoch::new(1))?,
    );

    // Update slashed balances
    let effective_balance = validator.effective_balance;
    let slashings_index = epoch.as_usize() % E::EpochsPerSlashingsVector::to_usize();
    let slashings = state
        .slashings_mut()
        .get_mut(slashings_index)
        .ok_or(Error::SlashingsOutOfBounds(slashings_index))?;
    *slashings = slashings.safe_add(effective_balance)?;

    // Apply slashing penalty
    decrease_balance(
        state,
        slashed_index,
        effective_balance
            .safe_div(spec.min_slashing_penalty_quotient_for_state(state))?,
    )?;

    // Need to call this to update caches
    update_progressive_balances_on_slashing(state, slashed_index, effective_balance)?;
    state
        .slashings_cache_mut()
        .record_validator_slashing(latest_block_slot, slashed_index)?;

    // No whistleblower rewards in our custom reward system
    // All rewards are managed centrally in per_block_processing.rs

    Ok(())
}
