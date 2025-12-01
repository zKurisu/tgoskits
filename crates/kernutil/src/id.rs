/// 批量定义 ID 类型
///
/// # 用法
/// ```ignore
/// define_ids! {
///     /// 任务 ID
///     TaskId(usize),
///     /// CPU ID
///     CpuId(u32),
///     /// 进程 ID
///     ProcessId(usize),
/// }
/// ```
#[macro_export]
macro_rules! define_ids {
    (
        $(
            $(#[$meta:meta])*
            $name:ident($inner_type:ty)
        ),* $(,)?
    ) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            #[repr(transparent)]
            pub struct $name($inner_type);

            impl $name {
                /// 创建新的 ID
                #[inline]
                pub const fn new(value: $inner_type) -> Self {
                    Self(value)
                }

                /// 获取内部值
                #[inline]
                pub const fn raw(&self) -> $inner_type {
                    self.0
                }
            }

            impl Default for $name {
                #[inline]
                fn default() -> Self {
                    Self(<$inner_type>::default())
                }
            }

            impl core::fmt::Display for $name {
                #[inline]
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}", self.0)
                }
            }

            impl core::fmt::Debug for $name {
                #[inline]
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    write!(f, "{}({:?})", stringify!($name), self.0)
                }
            }

            impl From<$inner_type> for $name {
                #[inline]
                fn from(value: $inner_type) -> Self {
                    Self(value)
                }
            }

            impl From<$name> for $inner_type {
                #[inline]
                fn from(id: $name) -> Self {
                    id.0
                }
            }

            impl core::ops::Add<$inner_type> for $name {
                type Output = Self;

                #[inline]
                fn add(self, rhs: $inner_type) -> Self::Output {
                    Self(self.0 + rhs)
                }
            }

            impl core::ops::Add<$name> for $name {
                type Output = Self;

                #[inline]
                fn add(self, rhs: $name) -> Self::Output {
                    Self(self.0 + rhs.0)
                }
            }

            impl core::ops::AddAssign<$inner_type> for $name {
                #[inline]
                fn add_assign(&mut self, rhs: $inner_type) {
                    self.0 += rhs;
                }
            }

            impl core::ops::Sub<$inner_type> for $name {
                type Output = Self;

                #[inline]
                fn sub(self, rhs: $inner_type) -> Self::Output {
                    Self(self.0 - rhs)
                }
            }

            impl core::ops::Sub<$name> for $name {
                type Output = Self;

                #[inline]
                fn sub(self, rhs: $name) -> Self::Output {
                    Self(self.0 - rhs.0)
                }
            }

            impl core::ops::SubAssign<$inner_type> for $name {
                #[inline]
                fn sub_assign(&mut self, rhs: $inner_type) {
                    self.0 -= rhs;
                }
            }
        )*
    };
}

#[cfg(test)]
mod tests {

    // 使用 define_ids! 批量定义测试用 ID 类型
    define_ids! {
        /// 测试用 ID
        TestId(usize),
        /// 支持负数的 ID
        NegId(isize),
    }

    #[test]
    fn test_basic_id_creation() {
        let id = TestId::new(42);
        assert_eq!(id.raw(), 42);
    }

    #[test]
    fn test_arithmetic_operations() {
        let id = TestId::new(10);

        // 加法
        assert_eq!((id + 5).raw(), 15);
        assert_eq!((id + TestId::new(3)).raw(), 13);

        // 减法
        assert_eq!((id - 3).raw(), 7);
        assert_eq!((id - TestId::new(2)).raw(), 8);
    }

    #[test]
    fn test_assignment_operations() {
        let mut id = TestId::new(10);

        id += 5;
        assert_eq!(id.raw(), 15);

        id -= 3;
        assert_eq!(id.raw(), 12);
    }

    #[test]
    fn test_comparisons() {
        let id1 = TestId::new(10);
        let id2 = TestId::new(20);
        let id3 = TestId::new(10);

        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert!(id1 < id2);
        assert!(id2 > id1);
        assert!(id1 <= id3);
        assert!(id1 >= id3);
    }
}
