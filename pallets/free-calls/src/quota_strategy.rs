use frame_support::pallet_prelude::Get;
use crate::quota::NumberOfCalls;
use pallet_locker_mirror::{BalanceOf, LockedInfo, LockedInfoOf};
use sp_std::cmp::min;
use sp_std::convert::TryInto;
use sp_std::marker::PhantomData;
use subsocial_primitives as primitives;
use subsocial_primitives::{AccountId, Balance, BlockNumber, currency};
use subsocial_primitives::time::*;
use crate::{Config, EligibleAccounts};

/// A strategy used to calculate the MaxQuota of a consumer.
pub trait MaxQuotaCalculationStrategy<AccountId, BlockNumber, Balance> {
    fn calculate(
        consumer: AccountId,
        current_block: BlockNumber,
        locked_info: Option<LockedInfo<BlockNumber, Balance>>,
    ) -> Option<NumberOfCalls>;
}


/// An implementation of the MaxQuotaCalculationStrategy that grants accounts in the eligible accounts
/// storage a fixed amount of free calls as max quota.
pub struct EligibleAccountsStrategy<T: Config>(PhantomData<T>);

impl<T: Config> Default for EligibleAccountsStrategy<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T: Config> MaxQuotaCalculationStrategy<T::AccountId, T::BlockNumber, BalanceOf<T>> for EligibleAccountsStrategy<T> {
    fn calculate(
        consumer: T::AccountId,
        _current_block: T::BlockNumber,
        _locked_info: Option<LockedInfoOf<T>>,
    ) -> Option<NumberOfCalls> {
        if EligibleAccounts::<T>::get(consumer) {
            Some(T::FreeQuotaPerEligibleAccount::get())
        } else {
            None
        }
    }
}

// TODO: try to find a better way to calculate it based on the circulating supply
const FREE_CALLS_PER_SUB: NumberOfCalls = 10;

/// A calculation strategy for free calls quota.
///
/// The calculation depends on the amount of tokens the user has locked and the time passed since
/// the lock. Each token locked will grant the user [FREE_CALLS_PER_SUB] to be used as free calls,
/// but the full amount will be not be fully accessible until the tokens have been locked for at
/// least 12 months. Before 12 months only a percentage of the free calls will be granted.
///
/// ```text
/// +-------------+------+---------+
/// | Time Passed | Days | Allowed |
/// +-------------+------+---------+
/// | Just Locked |    0 | 15%     |
/// | 1 week      |    7 | 30%     |
/// | 2 weeks     |   14 | 35%     |
/// | 3 weeks     |   21 | 40%     |
/// | 1 month     |   30 | 45%     |
/// | 2 months    |   60 | 50%     |
/// | 3 months    |   90 | 55%     |
/// | 4 months    |  120 | 60%     |
/// | 5 months    |  150 | 65%     |
/// | 6 months    |  180 | 70%     |
/// | 7 months    |  210 | 75%     |
/// | 8 months    |  240 | 80%     |
/// | 9 months    |  270 | 85%     |
/// | 10 months   |  300 | 90%     |
/// | 11 months   |  330 | 95%     |
/// | 12 months   |  360 | 100%    |
/// +-------------+------+---------+
/// ```
pub struct FreeCallsCalculationStrategy<AccountId, BlockNumber, Balance>(
    PhantomData<(AccountId, BlockNumber, Balance)>,
);

impl<AccountId, BlockNumber, Balance> Default for FreeCallsCalculationStrategy<AccountId, BlockNumber, Balance> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl MaxQuotaCalculationStrategy<
    primitives::AccountId,
    primitives::BlockNumber,
    primitives::Balance,
> for FreeCallsCalculationStrategy<
        primitives::AccountId,
        primitives::BlockNumber,
        primitives::Balance,
    >
{
    fn calculate(
        _consumer: primitives::AccountId,
        current_block: primitives::BlockNumber,
        locked_info: Option<LockedInfo<primitives::BlockNumber, primitives::Balance>>,
    ) -> Option<NumberOfCalls> {
        let LockedInfo::<primitives::BlockNumber, primitives::Balance> {
            locked_at,
            locked_amount,
            expires_at,
        } = match locked_info {
            Some(locked_info) => locked_info,
            None => return None,
        };

        if locked_at >= current_block {
            return None;
        }

        if matches!(expires_at, Some(expires_at) if current_block >= expires_at) {
            return None;
        }

        let time_locked = current_block - locked_at;

        let utilization_percent = get_utilization_percent(time_locked);

        let num_of_tokens = locked_amount.saturating_div(currency::DOLLARS) as u64;

        let num_of_free_calls = num_of_tokens
            .saturating_mul(FREE_CALLS_PER_SUB.into())
            .saturating_mul(utilization_percent)
            .saturating_div(100);

        if num_of_free_calls >= NumberOfCalls::MAX.into() {
            Some(NumberOfCalls::MAX)
        } else {
            Some(num_of_free_calls.try_into().unwrap_or(0))
        }
    }
}

fn get_utilization_percent(time_locked: primitives::BlockNumber) -> u64 {
    if time_locked < 1 * WEEKS {
        return 15;
    }
    if time_locked < 1 * MONTHS {
        let num_of_weeks = min(3, time_locked / (1 * WEEKS)) as u64;
        return (num_of_weeks * 5) + 25;
    }

    let num_of_months = min(12, time_locked / (1 * MONTHS)) as u64;
    return (num_of_months * 5) + 40;
}


#[cfg(test)]
mod tests {
    use frame_benchmarking::account;
    use pallet_locker_mirror::{LockedInfo, LockedInfoOf};
    use subsocial_primitives::{*, currency::*, time::*};
    use crate::quota::NumberOfCalls;
    use rstest::rstest;
    use crate::quota_strategy::FreeCallsCalculationStrategy;
    use super::MaxQuotaCalculationStrategy;

    #[rstest]
    // FREE_CALLS_PER_SUB = 10
    #[case(1 * CENTS, 10, Some(0))]

    #[case(1 * DOLLARS, 1 * DAYS, Some(1))]
    #[case(10 * DOLLARS, 1 * DAYS, Some(15))]
    #[case(100 * DOLLARS, 1 * DAYS, Some(150))]

    #[case(1 * DOLLARS, 1 * WEEKS, Some(3))]
    #[case(10 * DOLLARS, 1 * WEEKS, Some(30))]

    #[case(1 * DOLLARS, 2 * WEEKS, Some(3))]
    #[case(10 * DOLLARS, 2 * WEEKS, Some(35))]

    #[case(1 * DOLLARS, 3 * WEEKS, Some(4))]
    #[case(10 * DOLLARS, 3 * WEEKS, Some(40))]

    // 4 weeks (28) is treated as 3 weeks
    #[case(1 * DOLLARS, 4 * WEEKS, Some(4))]
    #[case(10 * DOLLARS, 4 * WEEKS, Some(40))]

    #[case(5 * DOLLARS, 1 * MONTHS, Some(22))]
    #[case(20 * DOLLARS, 1 * MONTHS, Some(90))]

    #[case(5 * DOLLARS, 2 * MONTHS, Some(25))]
    #[case(20 * DOLLARS, 2 * MONTHS, Some(100))]

    #[case(5 * DOLLARS, 3 * MONTHS, Some(27))]
    #[case(20 * DOLLARS, 3 * MONTHS, Some(110))]

    #[case(5 * DOLLARS, 4 * MONTHS, Some(30))]
    #[case(20 * DOLLARS, 4 * MONTHS, Some(120))]

    #[case(5 * DOLLARS, 5 * MONTHS, Some(32))]
    #[case(20 * DOLLARS, 5 * MONTHS, Some(130))]
    #[case(500 * DOLLARS, 5 * MONTHS, Some(3250))]

    // treated as 5 MONTHS
    #[case(500 * DOLLARS, 5 * MONTHS + 1 * WEEKS, Some(3250))]

    #[case(100 * DOLLARS, 6 * MONTHS, Some(700))]
    #[case(100 * DOLLARS, 7 * MONTHS, Some(750))]
    #[case(100 * DOLLARS, 8 * MONTHS, Some(800))]
    #[case(100 * DOLLARS, 9 * MONTHS, Some(850))]
    #[case(100 * DOLLARS, 10 * MONTHS, Some(900))]
    #[case(100 * DOLLARS, 11 * MONTHS, Some(950))]
    #[case(100 * DOLLARS, 12 * MONTHS, Some(1000))]

    #[case(100 * DOLLARS, 13 * MONTHS, Some(1000))]
    #[case(100 * DOLLARS, 100 * MONTHS, Some(1000))]
    #[trace]
    fn quota_calculation_strategy_tests(
        #[case] amount: Balance,
        #[case] lock_duration: BlockNumber,
        #[case] expected_quota: Option<NumberOfCalls>,
    ) {
        let current_block = 1000 * MONTHS;
        let before_current_block = current_block - 1;
        let after_current_block = current_block + 1;


        let locked_at = current_block - lock_duration;
        let locked_info = LockedInfo::<BlockNumber, Balance> {
            locked_at,
            locked_amount: amount.into(),
            expires_at: None,
        };

        let locked_info_not_yet_expired = {
            let mut locked_info = locked_info.clone();
            locked_info.expires_at = Some(after_current_block);
            locked_info
        };

        let locked_info_expired = {
            let mut locked_info = locked_info.clone();
            locked_info.expires_at = Some(before_current_block);
            locked_info
        };

        let locked_info_just_expired = {
            let mut locked_info = locked_info.clone();
            locked_info.expires_at = Some(current_block);
            locked_info
        };

        ///////////////////////////////////////
        let consumer = || account("Dummy Consumer", 0, 0);

        // Expect none if no locked_info provided
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(), current_block, None),
            None,
        );
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),before_current_block, None),
            None,
        );
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),after_current_block, None),
            None,
        );

        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),current_block, Some(locked_info)),
            expected_quota,
        );

        // test expiration
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),current_block, Some(locked_info_just_expired)),
            None,
        );
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),current_block, Some(locked_info_expired)),
            None,
        );
        assert_eq!(
            FreeCallsCalculationStrategy::calculate(consumer(),current_block, Some(locked_info_not_yet_expired)),
            expected_quota,
        );

    }
}