#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::string_lit_as_bytes)]

use codec::{Decode, Encode};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage,
    dispatch::{DispatchError, DispatchResult}, ensure, traits::Get,
};
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;
use system::ensure_signed;

use df_traits::{SpaceForRoles, SpaceProvider};
use df_traits::{PermissionChecker, SpaceFollowsProvider};
use pallet_permissions::{SpacePermission, SpacePermissions};
use pallet_permissions::SpacePermissionsContext;
use pallet_utils::{is_valid_handle_char, Module as Utils, SpaceId, WhoAndWhen};

// mod tests;

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct Space<T: Trait> {
    pub id: SpaceId,
    pub created: WhoAndWhen<T>,
    pub updated: Option<WhoAndWhen<T>>,
    pub hidden: bool,

    // Can be updated by the owner:
    pub owner: T::AccountId,
    pub handle: Option<Vec<u8>>,
    pub ipfs_hash: Vec<u8>,

    pub posts_count: u16,
    pub followers_count: u32,

    pub edit_history: Vec<SpaceHistoryRecord<T>>,

    pub score: i32,

    /// Allows to override the default permissions for this space.
    pub permissions: Option<SpacePermissions>,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
#[allow(clippy::option_option)]
pub struct SpaceUpdate {
    pub handle: Option<Option<Vec<u8>>>,
    pub ipfs_hash: Option<Vec<u8>>,
    pub hidden: Option<bool>,
}

#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct SpaceHistoryRecord<T: Trait> {
    pub edited: WhoAndWhen<T>,
    pub old_data: SpaceUpdate,
}

/// The pallet's configuration trait.
pub trait Trait: system::Trait
    + pallet_utils::Trait
{
    /// The overarching event type.
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;

    /// Minimal length of blog handle
    type MinHandleLen: Get<u32>;

    /// Maximal length of space handle
    type MaxHandleLen: Get<u32>;

    type Roles: PermissionChecker<AccountId=Self::AccountId>;

    type SpaceFollows: SpaceFollowsProvider<AccountId=Self::AccountId>;
}

decl_error! {
  pub enum Error for Module<T: Trait> {
    /// Space was not found by id.
    SpaceNotFound,
    /// Space handle is too short.
    HandleIsTooShort,
    /// Space handle is too long.
    HandleIsTooLong,
    /// Space handle is not unique.
    HandleIsNotUnique,
    /// Space handle contains invalid characters.
    HandleContainsInvalidChars,
    /// Nothing to update in space.
    NoUpdatesForSpace,
    /// Only space owner can manage their space.
    NotASpaceOwner,
    /// Overflow caused adding post to space
    PostsCountOverflow,
    /// User has no permission to update this space.
    NoPermissionToUpdateSpace,

    /// The current space owner cannot transfer ownership to himself.
    CannotTranferToCurrentOwner,
    /// There is no transfer ownership by space that is provided.
    NoPendingTransferOnSpace,
    /// The account is not allowed to accept transfer ownership.
    NotAllowedToAcceptOwnershipTransfer,
    /// The account is not allowed to reject transfer ownership.
    NotAllowedToRejectOwnershipTransfer,
  }
}

// This pallet's storage items.
decl_storage! {
  trait Store for Module<T: Trait> as TemplateModule {

    // TODO reserve space id 0 (zero) for 'Abyss'.

    pub NextSpaceId get(fn next_space_id): SpaceId = 1;
    pub SpaceById get(fn space_by_id): map SpaceId => Option<Space<T>>;
    pub SpaceIdByHandle get(fn space_id_by_handle): map Vec<u8> => Option<SpaceId>;
    pub SpaceIdsByOwner get(fn space_ids_by_owner): map T::AccountId => Vec<SpaceId>;
    pub PendingSpaceOwner get(fn pending_space_owner): map SpaceId => Option<T::AccountId>;
  }
}

decl_event!(
  pub enum Event<T> where
    <T as system::Trait>::AccountId,
  {
    SpaceCreated(AccountId, SpaceId),
    SpaceUpdated(AccountId, SpaceId),
    SpaceDeleted(AccountId, SpaceId),

    SpaceOwnershipTransferCreated(/* current owner */ AccountId, SpaceId, /* new owner */ AccountId),
    SpaceOwnershipTransferAccepted(AccountId, SpaceId),
    SpaceOwnershipTransferRejected(AccountId, SpaceId),
  }
);

// The pallet's dispatchable functions.
decl_module! {
  pub struct Module<T: Trait> for enum Call where origin: T::Origin {

    /// Minimal length of space handle
    const MinHandleLen: u32 = T::MinHandleLen::get();

    /// Maximal length of space handle
    const MaxHandleLen: u32 = T::MaxHandleLen::get();

    // Initializing events
    fn deposit_event() = default;

    pub fn create_space(origin, handle_opt: Option<Vec<u8>>, ipfs_hash: Vec<u8>) {
      let owner = ensure_signed(origin)?;

      Utils::<T>::is_ipfs_hash_valid(ipfs_hash.clone())?;

      let mut handle: Vec<u8> = Vec::new();
      if let Some(original_handle) = handle_opt.clone() {
        handle = Self::lowercase_and_validate_a_handle(original_handle)?;
      }

      let space_id = Self::next_space_id();
      let new_space = &mut Space::new(space_id, owner.clone(), ipfs_hash, handle_opt);

      // TODO old add_space_follower
      // Space creator automatically follows their space:
      // Self::add_space_follower(owner.clone(), new_space)?;

      if !handle.is_empty() {
        SpaceIdByHandle::insert(handle, space_id);
      }

      <SpaceById<T>>::insert(space_id, new_space);
      <SpaceIdsByOwner<T>>::mutate(owner.clone(), |ids| ids.push(space_id));
      NextSpaceId::mutate(|n| { *n += 1; });
      Self::deposit_event(RawEvent::SpaceCreated(owner, space_id));

      // TODO new add_space_follower
      // T::SpaceHandler::on_space_created(...);
    }

    pub fn update_space(origin, space_id: SpaceId, update: SpaceUpdate) {
      let owner = ensure_signed(origin)?;

      let has_updates =
        update.handle.is_some() ||
        update.ipfs_hash.is_some() ||
        update.hidden.is_some();

      ensure!(has_updates, Error::<T>::NoUpdatesForSpace);

      let mut space = Self::space_by_id(space_id).ok_or(Error::<T>::SpaceNotFound)?;

      Self::ensure_account_has_space_permission(
        owner.clone(),
        &space,
        SpacePermission::UpdateSpace,
        Error::<T>::NoPermissionToUpdateSpace.into()
      )?;

      let mut fields_updated = 0;
      let mut new_history_record = SpaceHistoryRecord {
        edited: WhoAndWhen::<T>::new(owner.clone()),
        old_data: SpaceUpdate {handle: None, ipfs_hash: None, hidden: None}
      };

      if let Some(ipfs_hash) = update.ipfs_hash {
        if ipfs_hash != space.ipfs_hash {
          Utils::<T>::is_ipfs_hash_valid(ipfs_hash.clone())?;
          new_history_record.old_data.ipfs_hash = Some(space.ipfs_hash);
          space.ipfs_hash = ipfs_hash;
          fields_updated += 1;
        }
      }

      if let Some(hidden) = update.hidden {
        if hidden != space.hidden {
          new_history_record.old_data.hidden = Some(space.hidden);
          space.hidden = hidden;
          fields_updated += 1;
        }
      }

      if let Some(handle_opt) = update.handle {
        if handle_opt != space.handle {
          if let Some(mut handle) = handle_opt.clone() {
            handle = Self::lowercase_and_validate_a_handle(handle)?;
            SpaceIdByHandle::insert(handle, space_id);
          }
          if let Some(space_handle) = space.handle.clone() {
            SpaceIdByHandle::remove(space_handle);
          }
          new_history_record.old_data.handle = Some(space.handle);
          space.handle = handle_opt;
          fields_updated += 1;
        }
      }

      // Update this space only if at least one field should be updated:
      if fields_updated > 0 {
        space.updated = Some(WhoAndWhen::<T>::new(owner.clone()));
        space.edit_history.push(new_history_record);
        <SpaceById<T>>::insert(space_id, space);
        Self::deposit_event(RawEvent::SpaceUpdated(owner, space_id));

        // TODO new
        // T::SpaceHandler::on_space_updated(...);
      }
    }

    pub fn transfer_space_ownership(origin, space_id: SpaceId, transfer_to: T::AccountId) {
      let who = ensure_signed(origin)?;

      let space = Self::space_by_id(space_id).ok_or(Error::<T>::SpaceNotFound)?;
      space.ensure_space_owner(who.clone())?;

      ensure!(who != transfer_to, Error::<T>::CannotTranferToCurrentOwner);
      Space::<T>::ensure_space_exists(space_id)?;

      <PendingSpaceOwner<T>>::insert(space_id, transfer_to.clone());
      Self::deposit_event(RawEvent::SpaceOwnershipTransferCreated(who, space_id, transfer_to));
    }

    pub fn accept_pending_ownership(origin, space_id: SpaceId) {
      let who = ensure_signed(origin)?;

      let transfer_to = Self::pending_space_owner(space_id).ok_or(Error::<T>::NoPendingTransferOnSpace)?;
      ensure!(who == transfer_to, Error::<T>::NotAllowedToAcceptOwnershipTransfer);

      // Here we know that the origin is eligible to become a new owner of this space.
      <PendingSpaceOwner<T>>::remove(space_id);

      if let Some(mut space) = Self::space_by_id(space_id) {
        space.owner = who.clone();
        <SpaceById<T>>::insert(space_id, space);
        Self::deposit_event(RawEvent::SpaceOwnershipTransferAccepted(who, space_id));

        // TODO new
        // T::SpaceHandler::on_new_space_owner(...);
      }
    }

    pub fn reject_pending_ownership(origin, space_id: SpaceId) {
      let who = ensure_signed(origin)?;

      let space = Self::space_by_id(space_id).ok_or(Error::<T>::SpaceNotFound)?;
      let transfer_to = Self::pending_space_owner(space_id).ok_or(Error::<T>::NoPendingTransferOnSpace)?;
      ensure!(who == transfer_to || who == space.owner, Error::<T>::NotAllowedToRejectOwnershipTransfer);

      <PendingSpaceOwner<T>>::remove(space_id);
      Self::deposit_event(RawEvent::SpaceOwnershipTransferRejected(who, space_id));
    }
  }
}

impl<T: Trait> Space<T> {
    pub fn new(
        id: SpaceId,
        created_by: T::AccountId,
        ipfs_hash: Vec<u8>,
        handle: Option<Vec<u8>>,
    ) -> Self {
        Space {
            id,
            created: WhoAndWhen::<T>::new(created_by.clone()),
            updated: None,
            hidden: false,
            owner: created_by,
            handle,
            ipfs_hash,
            posts_count: 0,
            followers_count: 0,
            edit_history: Vec::new(),
            score: 0,
            permissions: None,
        }
    }

    pub fn is_owner(&self, account: &T::AccountId) -> bool {
        self.owner == *account
    }

    pub fn increment_posts_count(&mut self) -> DispatchResult {
        self.posts_count = self.posts_count.checked_add(1).ok_or(Error::<T>::PostsCountOverflow)?;
        Ok(())
    }

    pub fn ensure_space_owner(&self, who: T::AccountId) -> DispatchResult {
        ensure!(self.is_owner(&who), Error::<T>::NotASpaceOwner);
        Ok(())
    }

    pub fn ensure_space_exists(space_id: SpaceId) -> DispatchResult {
        ensure!(<SpaceById<T>>::exists(space_id), Error::<T>::SpaceNotFound);
        Ok(())
    }
}

impl<T: Trait> Module<T> {

    pub fn lowercase_and_validate_a_handle(mut handle: Vec<u8>) -> Result<Vec<u8>, DispatchError> {
        handle = handle.to_ascii_lowercase();

        ensure!(Self::space_id_by_handle(handle.clone()).is_none(), Error::<T>::HandleIsNotUnique);
        ensure!(handle.len() >= T::MinHandleLen::get() as usize, Error::<T>::HandleIsTooShort);
        ensure!(handle.len() <= T::MaxHandleLen::get() as usize, Error::<T>::HandleIsTooLong);
        ensure!(handle.iter().all(|&x| is_valid_handle_char(x)), Error::<T>::HandleContainsInvalidChars);

        Ok(handle)
    }

    pub fn ensure_account_has_space_permission(
        account: T::AccountId,
        space: &Space<T>,
        permission: SpacePermission,
        error: DispatchError,
    ) -> DispatchResult {
        let is_owner = space.is_owner(&account);
        let is_follower = T::SpaceFollows::is_space_follower(account.clone(), space.id);

        let ctx = SpacePermissionsContext {
            space_id: space.id,
            is_space_owner: is_owner,
            is_space_follower: is_follower,
            space_perms: space.permissions.clone(),
        };

        T::Roles::ensure_account_has_space_permission(
            account,
            ctx,
            permission,
            error,
        )
    }
}

impl<T: Trait> SpaceProvider for Module<T> {
    type AccountId = T::AccountId;

    fn get_space(id: SpaceId) -> Result<SpaceForRoles<Self::AccountId>, DispatchError> {
        let space: Space<T> = Module::space_by_id(id).ok_or(Error::<T>::SpaceNotFound)?;

        Ok(SpaceForRoles {
            owner: space.owner,
            permissions: space.permissions,
        })
    }
}
