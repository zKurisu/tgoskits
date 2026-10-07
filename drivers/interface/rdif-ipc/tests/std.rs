//! 接口层的组装与默认行为：用一个只记账的假后端证明
//! ① 后端能通过 `Ipc` 句柄被调用（`def_driver!` 的分发与 `Deref`），
//! ② 错误码原样传出来，③ 不覆写就拿到 `remote_state` / `shared_window` 的默认值。
//!
//! 这里刻意**不**测真实硬件语义——那是后端自己的测试与板测用例负责的。

extern crate alloc;

use alloc::{vec, vec::Vec};

use rdif_ipc::{DriverGeneric, Interface, Ipc, IpcError, IpcMessage, RemoteState, SharedWindow};

struct FakeIpc {
    sent: Vec<IpcMessage>,
    inbox: Vec<IpcMessage>,
    pumps: usize,
    fail_send: Option<IpcError>,
}

impl FakeIpc {
    fn new() -> Self {
        Self {
            sent: Vec::new(),
            inbox: Vec::new(),
            pumps: 0,
            fail_send: None,
        }
    }
}

impl DriverGeneric for FakeIpc {
    fn name(&self) -> &str {
        "fake-ipc"
    }
}

impl Interface for FakeIpc {
    fn send(&mut self, msg: IpcMessage) -> Result<(), IpcError> {
        if let Some(err) = self.fail_send {
            return Err(err);
        }
        self.sent.push(msg);
        Ok(())
    }

    fn pump(&mut self) {
        self.pumps += 1;
    }

    fn try_recv(&mut self) -> Option<IpcMessage> {
        if self.inbox.is_empty() {
            None
        } else {
            Some(self.inbox.remove(0))
        }
    }

    fn is_ready(&self) -> bool {
        !self.inbox.is_empty()
    }
}

#[test]
fn send_goes_through_the_handle() {
    let mut ipc = Ipc::new(FakeIpc::new());
    let msg = IpcMessage::new(6, 0x60, 200);

    ipc.send(msg).unwrap();

    assert_eq!(ipc.typed_ref::<FakeIpc>().unwrap().sent, vec![msg]);
}

#[test]
fn backend_errors_surface_unchanged() {
    let mut fake = FakeIpc::new();
    fake.fail_send = Some(IpcError::WouldBlock);
    let mut ipc = Ipc::new(fake);

    assert_eq!(
        ipc.send(IpcMessage::new(6, 0x60, 0)),
        Err(IpcError::WouldBlock)
    );
}

#[test]
fn queue_readiness_and_receive_reflect_backend_state() {
    let mut fake = FakeIpc::new();
    let inbound = IpcMessage::new(6, 0x52, 7);
    fake.inbox.push(inbound);
    let mut ipc = Ipc::new(fake);

    assert!(ipc.is_ready());
    ipc.pump();
    assert_eq!(ipc.try_recv(), Some(inbound));
    assert!(!ipc.is_ready());
    assert_eq!(ipc.try_recv(), None);
    assert_eq!(ipc.typed_ref::<FakeIpc>().unwrap().pumps, 1);
}

#[test]
fn optional_capabilities_default_to_absent() {
    let ipc = Ipc::new(FakeIpc::new());

    assert_eq!(ipc.remote_state(), RemoteState::Unknown);
    assert_eq!(ipc.shared_window(), None);
}

#[test]
fn optional_capabilities_can_be_provided() {
    struct WithWindow;

    impl DriverGeneric for WithWindow {
        fn name(&self) -> &str {
            "with-window"
        }
    }

    impl Interface for WithWindow {
        fn send(&mut self, _msg: IpcMessage) -> Result<(), IpcError> {
            Ok(())
        }
        fn pump(&mut self) {}
        fn try_recv(&mut self) -> Option<IpcMessage> {
            None
        }
        fn is_ready(&self) -> bool {
            false
        }
        fn remote_state(&self) -> RemoteState {
            RemoteState::Running
        }
        fn shared_window(&self) -> Option<SharedWindow> {
            Some(SharedWindow {
                paddr: 0x8b20_0000,
                size: 0x10_0000,
            })
        }
    }

    let ipc = Ipc::new(WithWindow);

    assert_eq!(ipc.remote_state(), RemoteState::Running);
    assert_eq!(
        ipc.shared_window(),
        Some(SharedWindow {
            paddr: 0x8b20_0000,
            size: 0x10_0000
        })
    );
}
