use crate::memory::{StableBlockOrders, StablePrincipalOrders, StableUserOrders, NEW_USER_ORDERS};
use candid::Principal;

use dod_utils::types::{BlockNumber, BlockRange, NewBlockOrderValue, OrderDetail, OrderStatus};
use ic_cdk::id;

pub struct NewBlockOrders {}

impl NewBlockOrders {
    /// Writes an order by block height.
    ///
    /// This function inserts an order into the `StableBlockOrders` for a specified block number and user.
    /// The order details include the value and status of the order.
    ///
    /// # Arguments
    ///
    /// * `block_orders` - A mutable reference to `StableBlockOrders` where the order will be inserted.
    /// * `block_number` - A `BlockNumber` representing the block height.
    /// * `user_id` - A `Principal` representing the user placing the order.
    /// * `value` - A `u128` representing the value of the order.
    /// * `status` - An `OrderStatus` representing the status of the order.
    ///
    /// # Returns
    ///
    /// * `Option<OrderDetail>` - Returns the previous order detail if it existed, otherwise `None`.
    pub fn write_order_by_block_height(
        block_orders: &mut StableBlockOrders,
        block_number: BlockNumber,
        user_id: Principal,
        value: u128,
        status: OrderStatus,
    ) -> Option<OrderDetail> {
        block_orders.insert((block_number, user_id), OrderDetail { value, status })
    }

    /// Removes an order by block height.
    ///
    /// This function removes an order from the `StableBlockOrders` for a specified block number and user.
    ///
    /// # Arguments
    ///
    /// * `block_orders` - A mutable reference to `StableBlockOrders` from which the order will be removed.
    /// * `block_number` - A `BlockNumber` representing the block height.
    /// * `user_id` - A `Principal` representing the user whose order will be removed.
    ///
    /// # Returns
    ///
    /// * `Option<OrderDetail>` - Returns the removed order detail if it existed, otherwise `None`.
    pub fn remove_order_by_block_height(
        block_orders: &mut StableBlockOrders,
        block_number: BlockNumber,
        user_id: Principal,
    ) -> Option<OrderDetail> {
        block_orders.remove(&(block_number, user_id))
    }

    /// Retrieves orders by block height.
    ///
    /// This function returns an iterator over the orders in the `StableBlockOrders` for a specified block number.
    /// It filters the orders to include only those that belong to users with active bets or the current user.
    ///
    /// # Arguments
    ///
    /// * `block_orders` - A reference to `StableBlockOrders` containing the orders.
    /// * `block_number` - A `BlockNumber` representing the block height.
    ///
    /// # Returns
    ///
    /// * `impl Iterator<Item = (Principal, OrderDetail)> + '_` - An iterator over the orders for the specified block height.
    pub fn get_orders_by_block_height(
        block_orders: &StableBlockOrders,
        block_number: BlockNumber,
    ) -> impl Iterator<Item = (Principal, OrderDetail)> + '_ {
        block_orders
            .range((block_number, Principal::anonymous())..)
            .take_while(move |(r, _)| r.0 == block_number)
            .filter(move |&((b, r), _)| NewUserOrders::get_user_bet(r, b).is_some() || r == id())
            .map(|((_, s), t)| (s, t))
    }

    /// Writes a principal order by block height.
    ///
    /// This function inserts an order into the `StablePrincipalOrders` for a specified block number and user.
    /// The order details include the value and status of the order.
    ///
    /// # Arguments
    ///
    /// * `principal_orders` - A mutable reference to `StablePrincipalOrders` where the order will be inserted.
    /// * `block_number` - A `BlockNumber` representing the block height.
    /// * `user_id` - A `Principal` representing the user placing the order.
    /// * `value` - A `u128` representing the value of the order.
    /// * `status` - An `OrderStatus` representing the status of the order.
    ///
    /// # Returns
    ///
    /// * `Option<OrderDetail>` - Returns the previous order detail if it existed, otherwise `None`.
    pub fn write_p_order_by_block_height(
        principal_orders: &mut StablePrincipalOrders,
        block_number: BlockNumber,
        user_id: Principal,
        value: u128,
        status: OrderStatus,
    ) -> Option<OrderDetail> {
        principal_orders.insert((user_id, block_number), OrderDetail { value, status })
    }

    /// Retrieves orders within a specified block range.
    ///
    /// This function returns an iterator over the orders in the `StableBlockOrders`
    /// within the specified block range. Each item in the iterator is a tuple containing
    /// the block number, the principal of the user, and the order details.
    ///
    /// # Arguments
    ///
    /// * `block_orders` - A reference to `StableBlockOrders` containing the orders.
    /// * `range` - A `BlockRange` representing the start and end block heights.
    ///
    /// # Returns
    ///
    /// * `impl Iterator<Item = (u64, (Principal, OrderDetail))> + '_` - An iterator over the orders within the specified block range.
    pub fn get_orders_in_range(
        block_orders: &StableBlockOrders,
        range: BlockRange,
    ) -> impl Iterator<Item = (u64, (Principal, OrderDetail))> + '_ {
        block_orders
            .range((range.0, Principal::anonymous())..=(range.1, Principal::anonymous()))
            .map(|((block_number, p), v)| (block_number, (p, v)))
    }

    /// Retrieves user orders within a specified block range.
    ///
    /// This function returns an iterator over the orders in the `StableBlockOrders`
    /// for a specified user within the given block range. Each item in the iterator
    /// is a tuple containing the block number and the order details.
    ///
    /// # Arguments
    ///
    /// * `block_orders` - A reference to `StableBlockOrders` containing the orders.
    /// * `user_id` - A `Principal` representing the user whose orders will be retrieved.
    /// * `range` - A `BlockRange` representing the start and end block heights.
    ///
    /// # Returns
    ///
    /// * `impl Iterator<Item = (u64, OrderDetail)> + '_` - An iterator over the user's orders within the specified block range.
    pub fn get_user_orders_in_range(
        block_orders: &StableBlockOrders,
        user_id: Principal,
        range: BlockRange,
    ) -> impl Iterator<Item = (u64, OrderDetail)> + '_ {
        block_orders
            .range((range.0, user_id)..=(range.1, user_id))
            .filter(move |&((b, r), _)| {
                NewUserOrders::get_user_bet(user_id, b).is_some() && r == user_id
            })
            .map(|((block_number, _), v)| (block_number, v))
    }

    /// Retrieves principal orders within a specified block range.
    ///
    /// This function returns an iterator over the orders in the `StablePrincipalOrders`
    /// for a specified user within the given block range. Each item in the iterator
    /// is a tuple containing the block number and the order details.
    ///
    /// # Arguments
    ///
    /// * `principal_orders` - A reference to `StablePrincipalOrders` containing the orders.
    /// * `user_id` - A `Principal` representing the user whose orders will be retrieved.
    /// * `range` - A `BlockRange` representing the start and end block heights.
    ///
    /// # Returns
    ///
    /// * `impl Iterator<Item = (u64, OrderDetail)> + '_` - An iterator over the user's orders within the specified block range.
    pub fn get_p_orders_in_range(
        principal_orders: &StablePrincipalOrders,
        user_id: Principal,
        range: BlockRange,
    ) -> impl Iterator<Item = (u64, OrderDetail)> + '_ {
        principal_orders
            .range((user_id, range.0)..=(user_id, range.1))
            // .take_while(move |&((r, _), _)| r == user_id)
            .map(|((_, block_number), v)| (block_number, v))
    }
}

pub struct NewUserOrders {}

impl NewUserOrders {
    // 修改用户订单
    /// Updates a user's order.
    ///
    /// This function updates the order for a specified user in the `StableUserOrders`.
    /// Each user is allowed only one betting range, and this function will overwrite any existing strategy.
    ///
    /// # Arguments
    ///
    /// * `user_orders` - A mutable reference to `StableUserOrders` where the order will be updated.
    /// * `user_id` - A `Principal` representing the user whose order will be updated.
    /// * `range` - A `BlockRange` representing the start and end block heights for the order.
    /// * `amount` - A `u128` representing the amount of the order.
    pub fn update_order(
        user_orders: &mut StableUserOrders,
        user_id: Principal,
        range: BlockRange,
        amount: u128,
    ) {
        // 每个用户只允许有一个��注范围，直接覆盖旧的策略
        user_orders.insert(
            user_id,
            NewBlockOrderValue {
                r: range,
                v: amount,
            },
        );
    }

    // 查询用户在某个区块是否有订单
    /// Queries if a user has an order for a specific block number.
    ///
    /// This function checks if a user has an order within a specified block number.
    /// It returns the amount of the order if it exists and the block number is within the user's range.
    ///
    /// # Arguments
    ///
    /// * `user_id` - A `Principal` representing the user whose order is being queried.
    /// * `block_number` - A `BlockNumber` representing the block height to check.
    ///
    /// # Returns
    ///
    /// * `Option<u128>` - Returns the amount of the order if it exists and is within the range, otherwise `None`.
    pub fn get_user_bet(user_id: Principal, block_number: BlockNumber) -> Option<u128> {
        NEW_USER_ORDERS.with_borrow(|user_orders| {
            if let Some(NewBlockOrderValue {
                r: range,
                v: amount,
            }) = user_orders.get(&user_id)
            {
                if block_number < range.1 {
                    return Some(amount);
                }
            }
            None
        })
    }

    /// Retrieves the order range set by a user.
    ///
    /// This function returns the order range set by a specified user in the `NEW_USER_ORDERS`.
    ///
    /// # Arguments
    ///
    /// * `user_id` - A `Principal` representing the user whose order range is being queried.
    ///
    /// # Returns
    ///
    /// * `Option<NewBlockOrderValue>` - Returns the order range set by the user if it exists, otherwise `None`.
    pub fn get_user_set_range(user_id: Principal) -> Option<NewBlockOrderValue> {
        NEW_USER_ORDERS.with_borrow(|user_orders| user_orders.get(&user_id))
    }
}

#[cfg(test)]
mod test {
    use crate::memory::{NEW_BLOCK_ORDERS, NEW_USER_ORDERS};
    use crate::orders::{NewBlockOrders, NewUserOrders};
    use candid::Principal;
    use dod_utils::types::{OrderDetail, OrderStatus};

    #[test]
    pub fn test_range() {
        let p1 = Principal::from_text("bkyz2-fmaaa-aaaaa-qaaaq-cai").unwrap();
        let p2 = Principal::from_text("tmhkz-dyaaa-aaaah-aedeq-cai").unwrap();

        NEW_USER_ORDERS.with_borrow_mut(|v| {
            NewUserOrders::update_order(v, p1, (1, 2), 100);
        });

        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 1, p1, 100, OrderStatus::Pending);
        });
        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 2, p1, 60, OrderStatus::Pending);
        });

        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 3, p1, 100, OrderStatus::Pending);
        });
        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 4, p1, 60, OrderStatus::Pending);
        });
        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 2, p2, 30, OrderStatus::Pending);
        });
        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 4, p2, 10, OrderStatus::Pending);
        });

        NEW_BLOCK_ORDERS.with_borrow_mut(|v| {
            NewBlockOrders::write_order_by_block_height(v, 1, p2, 50, OrderStatus::Pending);
        });

        NEW_BLOCK_ORDERS.with_borrow(|v| {
            let d = NewBlockOrders::get_user_orders_in_range(&v, p1, (3, 4))
                .map(|(v, j)| (v, j))
                .collect::<Vec<(u64, OrderDetail)>>();
            assert_eq!(d, vec![]);
        })
    }
}
