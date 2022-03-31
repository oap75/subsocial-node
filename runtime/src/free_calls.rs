//! All related code to free-calls module

use frame_support::log::{debug, info};
use sp_std::convert::TryInto;
use frame_support::traits::Contains;
use sp_std::cmp::min;
use sp_std::if_std;
use static_assertions::const_assert;
use pallet_free_calls::max_quota_percentage;
use pallet_free_calls::config::WindowConfig;
use pallet_free_calls::quota::{QUOTA_PRECISION, NumberOfCalls};
use pallet_locker_mirror::LockedInfoOf;
use crate::BlockNumber;
use subsocial_primitives::time::*;
use subsocial_primitives::currency;
use super::{Runtime, Call};

pub const FREE_CALLS_WINDOWS_CONFIGS: [WindowConfig<BlockNumber>; 3] = [
    WindowConfig::new(1 * DAYS, max_quota_percentage!(100)),
    WindowConfig::new(1 * HOURS, max_quota_percentage!(30)),
    WindowConfig::new(5 * MINUTES, max_quota_percentage!(10)),
];


// Assert at compile time that the free-calls configs are in the optimal shape.
const_assert!(check_free_calls_config(&FREE_CALLS_WINDOWS_CONFIGS));
#[allow(dead_code)] // the code is not acutely dead.
const fn check_free_calls_config(configs: &'static [WindowConfig<BlockNumber>]) -> bool {
    // cannot have empty configs
    if configs.is_empty() {
        return false;
    }

    let mut prev_config = &configs[0];
    // A fraction of the first config should be equal to the quota precision.
    if prev_config.fraction_of_max_quota.get() != QUOTA_PRECISION {
        return false;
    }

    let mut i = 1;

    while i < configs.len() {
        let current_config = &configs[i];

        if current_config.period >= prev_config.period {
            return false;
        }

        if current_config.fraction_of_max_quota.get() >= prev_config.fraction_of_max_quota.get() {
            return false;
        }

        prev_config = current_config;
        i = i + 1;
    }

    return true;
}

/// Filter the calls that can be used as free calls.
// TODO: add more calls to this filter. or maybe allow all calls???
pub struct FreeCallsFilter;
impl Default for FreeCallsFilter { fn default() -> Self { Self } }
impl Contains<Call> for FreeCallsFilter {
    fn contains(c: &Call) -> bool {
        match *c {
            Call::Posts(..) => true,
            Call::Profiles(..) => true,
            Call::ProfileFollows(..) => true,
            Call::Roles(..) => true,
            Call::Spaces(..) => true,
            Call::SpaceFollows(..) => true,
            Call::Reactions(..) => true,
            Call::SpaceOwnership(..) => true,
            // Call::Moderation(..) => true,
            Call::System(..) => cfg!(feature = "runtime-benchmarks"),
            _ => false,
        }
    }
}