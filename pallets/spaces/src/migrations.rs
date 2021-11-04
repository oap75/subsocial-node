use frame_support::storage::IterableStorageMap;
use sp_runtime::traits::Saturating;

use pallet_utils as Utils;

use super::*;

pub fn fix_corrupted_handles_storage<T: Config>() -> frame_support::weights::Weight {
  let mut handles_to_remove = Vec::new();
  let mut handles_iterated = 0;

  for (handle, space_id) in SpaceIdByHandle::iter() {
    handles_iterated += 1;

    if let Some(space) = Module::<T>::space_by_id(&space_id) {
      let handle_lowered = space.handle.map(Utils::Module::<T>::lowercase_handle);

      if handle_lowered.is_none() || handle_lowered.as_ref() != Some(&handle) {
        handles_to_remove.push(handle.clone());
      }
    } else {
      handles_to_remove.push(handle.clone());
    }
  }

  for handle in &handles_to_remove {
    SpaceIdByHandle::remove(handle);
  }

  SpaceIdByHandleStorageFixed::put(true);

  T::DbWeight::get().reads_writes(
    handles_iterated.saturating_mul(2),
    handles_to_remove.len() as u64 + 1,
  )
}
