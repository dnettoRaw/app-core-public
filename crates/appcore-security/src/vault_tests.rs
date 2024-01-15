// =============================================================================
//        #######
//     ###       ###     F: vault_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{Vault, VaultState};
use crate::token::SecurityResult;

struct MockVault {
    state: VaultState,
}

impl Vault for MockVault {
    fn state(&self) -> VaultState {
        self.state
    }

    fn lock(&mut self) -> SecurityResult<()> {
        self.state = VaultState::Locked;
        Ok(())
    }

    fn unlock(&mut self, _key_material: &[u8]) -> SecurityResult<()> {
        self.state = VaultState::Unlocked;
        Ok(())
    }
}

#[test]
fn mock_vault_lock_unlock_works() {
    let mut vault = MockVault {
        state: VaultState::Locked,
    };
    assert_eq!(vault.state(), VaultState::Locked);
    assert!(vault.unlock(b"k").is_ok());
    assert_eq!(vault.state(), VaultState::Unlocked);
    assert!(vault.lock().is_ok());
    assert_eq!(vault.state(), VaultState::Locked);
}
