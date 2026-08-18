/// Send a key down + key up event via the Windows SendInput API.
pub fn send_key(vk: u16) {
    #[repr(C)]
    struct RawInput {
        ty: u32,
        _pad0: u32,
        vk: u16,
        scan: u16,
        flags: u32,
        time: u32,
        _pad1: u32,
        extra: usize,
        _tail: [u8; 8],
    }
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *mut RawInput, size: i32) -> u32;
    }
    const INPUT_KEYBOARD: u32 = 1;
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    let mk = |flags: u32| RawInput {
        ty: INPUT_KEYBOARD,
        _pad0: 0,
        vk,
        scan: 0,
        flags,
        time: 0,
        _pad1: 0,
        extra: 0,
        _tail: [0; 8],
    };
    let mut events = [mk(0), mk(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(
            events.len() as u32,
            events.as_mut_ptr(),
            std::mem::size_of::<RawInput>() as i32,
        );
    }
}
