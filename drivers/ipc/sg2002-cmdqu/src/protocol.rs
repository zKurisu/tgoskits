//! `cmdqu` 信封的编解码与合法性校验。
//!
//! 协议定义来自厂商头文件 `rtos_cmdqu.h`：一个信封占信箱的一个槽位，共 8 字节；
//! 前 4 字节按位打包（小端），后 4 字节是 `param_ptr`：
//!
//! ```text
//! bit  0..7   ip_id        子系统号
//! bit  8..14  cmd_id       子系统内命令号（7 位）
//! bit 15      block        是否要求对端回信
//! bit 16..23  linux_valid  大核写、小核读的方向标记
//! bit 24..31  rtos_valid   小核写、大核读的方向标记
//! ```

/// 槽位宽度（字节）。
pub const SLOT_SIZE: usize = 8;
/// 槽位数量。
pub const SLOT_COUNT: usize = 8;
/// `cmd_id` 只有 7 位。
pub const CMD_ID_MAX: u8 = 0x7f;
/// 子系统号上限（厂商头文件里的 `IP_LIMIT`）。
pub const IP_ID_LIMIT: u8 = 8;
/// 系统类命令号上限（厂商头文件里的 `SYS_CMD_INFO_LIMIT`）。
pub const SYS_CMD_ID_LIMIT: u8 = 0x60;

/// 子系统编号，与厂商 `enum IP_TYPE` 逐项对应。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IpId {
    Isp    = 0,
    Vcodec = 1,
    Vip    = 2,
    Vi     = 3,
    Rgn    = 4,
    Audio  = 5,
    System = 6,
    Camera = 7,
}

/// 系统类命令号，取自厂商 `enum SYS_CMD_ID` 的常用子集。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SysCmdId {
    /// 通用转发：小核侧没有专门分支，会走兜底回显。
    InfoTrans     = 0x50,
    /// 大核宣告自己已就绪。
    LinuxInitDone = 0x51,
    /// 小核宣告自己已就绪，并把 `transfer_config` 地址回传。
    RtosInitDone  = 0x52,
    /// 停中断请求。
    StopIsr       = 0x53,
    /// 停中断完成。
    StopIsrDone   = 0x54,
}

/// 编解码失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// 子系统号超出 `IP_ID_LIMIT`。
    IpIdOutOfRange(u8),
    /// 命令号超出 7 位可表示范围。
    CmdIdOutOfRange(u8),
}

/// 一个 8 字节信封。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Envelope {
    /// 子系统号。
    pub ip_id: u8,
    /// 子系统内命令号（7 位）。
    pub cmd_id: u8,
    /// 是否要求对端回信。
    pub block: bool,
    /// 大核→小核方向标记。
    pub linux_valid: bool,
    /// 小核→大核方向标记。
    pub rtos_valid: bool,
    /// 载荷指针或一个小整数参数。
    pub param_ptr: u32,
}

impl Envelope {
    /// 构造一条由大核发出的请求（`linux_valid = 1`、`rtos_valid = 0`）。
    pub const fn request(ip_id: IpId, cmd_id: u8, block: bool, param_ptr: u32) -> Self {
        Self::request_raw(ip_id as u8, cmd_id, block, param_ptr)
    }

    /// 与 [`Envelope::request`] 相同，但子系统号用裸 `u8`（来自用户态时更直接）。
    pub const fn request_raw(ip_id: u8, cmd_id: u8, block: bool, param_ptr: u32) -> Self {
        Self {
            ip_id,
            cmd_id,
            block,
            linux_valid: true,
            rtos_valid: false,
            param_ptr,
        }
    }

    /// 打包头部 4 字节。
    pub const fn header_word(&self) -> u32 {
        (self.ip_id as u32)
            | ((self.cmd_id as u32) << 8)
            | ((self.block as u32) << 15)
            | ((self.linux_valid as u32) << 16)
            | ((self.rtos_valid as u32) << 24)
    }

    /// 打包成 `(头部字, param_ptr)`，等同于槽位里的两次 32 位写。
    pub const fn words(&self) -> (u32, u32) {
        (self.header_word(), self.param_ptr)
    }

    /// 从槽位里的两个 32 位字解出信封。
    pub const fn from_words(header: u32, param_ptr: u32) -> Self {
        Self {
            ip_id: (header & 0xff) as u8,
            cmd_id: ((header >> 8) & 0x7f) as u8,
            block: (header >> 15) & 1 == 1,
            linux_valid: (header >> 16) & 1 == 1,
            rtos_valid: (header >> 24) & 1 == 1,
            param_ptr,
        }
    }

    /// 校验取值范围：子系统号必须小于 `IP_ID_LIMIT`，命令号必须落在 7 位内。
    pub const fn validate(&self) -> Result<(), EncodeError> {
        if self.ip_id >= IP_ID_LIMIT {
            return Err(EncodeError::IpIdOutOfRange(self.ip_id));
        }
        if self.cmd_id > CMD_ID_MAX {
            return Err(EncodeError::CmdIdOutOfRange(self.cmd_id));
        }
        Ok(())
    }

    /// 这条信封是否来自小核（对端已应答）。
    pub const fn is_reply(&self) -> bool {
        self.rtos_valid
    }

    /// 槽位是否为空：两个方向标记都为 0 才表示空闲。
    pub const fn is_empty_slot(header: u32, param_ptr: u32) -> bool {
        // 空槽的两个有效位都是 0，因此头部的第 16、24 位为 0；
        // 硬件会把整个槽位清零，直接看两个 32 位字更快。
        header == 0 && param_ptr == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_bit_positions_match_vendor_offsets() {
        let env = Envelope {
            ip_id: 0xab,
            cmd_id: 0x55,
            block: true,
            linux_valid: true,
            rtos_valid: true,
            param_ptr: 0,
        };
        assert_eq!(env.header_word(), 0x0101_d5ab);
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let env = Envelope::request(IpId::Vcodec, 0x21, true, 0x8fe1_d400);
        let (header, param) = env.words();
        assert_eq!(Envelope::from_words(header, param), env);
    }

    #[test]
    fn rtos_reply_is_detected() {
        // 小核回信：ip_id=6、cmd_id=0x52、block=0、rtos_valid=1（实板实测值）
        let reply = Envelope::from_words(0x0101_5206, 0x8fe1_d400);
        assert_eq!(reply.ip_id, IpId::System as u8);
        assert_eq!(reply.cmd_id, SysCmdId::RtosInitDone as u8);
        assert!(!reply.block);
        assert!(reply.is_reply());
        assert_eq!(reply.param_ptr, 0x8fe1_d400);
    }

    #[test]
    fn validate_rejects_out_of_range_values() {
        let bad_ip = Envelope::request(IpId::System, 0x50, false, 0);
        assert_eq!(bad_ip.validate(), Ok(()));

        let mut env = bad_ip;
        env.ip_id = IP_ID_LIMIT;
        assert_eq!(
            env.validate(),
            Err(EncodeError::IpIdOutOfRange(IP_ID_LIMIT))
        );

        let mut env = bad_ip;
        env.cmd_id = 0x80;
        assert_eq!(env.validate(), Err(EncodeError::CmdIdOutOfRange(0x80)));
    }

    #[test]
    fn empty_slot_detection() {
        assert!(Envelope::is_empty_slot(0, 0));
        assert!(!Envelope::is_empty_slot(0x0001_0006, 0));
    }
}
