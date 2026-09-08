use super::{hiword, loword};

#[test]
fn wm_command_words_are_decoded_by_contract() {
    let packed = (0x1234usize << 16) | 0x0056;
    assert_eq!(loword(packed), 0x0056);
    assert_eq!(hiword(packed), 0x1234);
}
