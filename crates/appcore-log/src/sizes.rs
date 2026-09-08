// =============================================================================
//        #######
//     ###       ###     F: sizes.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Common binary size limits for file and encrypted log sinks.

/// One mebibyte, suitable for small local or short-lived logs.
pub const LOG_SIZE_1_MIB: u64 = 1024 * 1024;
/// Two mebibytes, suitable for small application crash logs.
pub const LOG_SIZE_2_MIB: u64 = 2 * LOG_SIZE_1_MIB;
/// Four mebibytes, suitable for modest local application logs.
pub const LOG_SIZE_4_MIB: u64 = 4 * LOG_SIZE_1_MIB;
/// Eight mebibytes, suitable for normal rotating application logs.
pub const LOG_SIZE_8_MIB: u64 = 8 * LOG_SIZE_1_MIB;
/// Sixteen mebibytes, suitable for moderate operational logs.
pub const LOG_SIZE_16_MIB: u64 = 16 * LOG_SIZE_1_MIB;
/// Thirty-two mebibytes, suitable for busier bounded services.
pub const LOG_SIZE_32_MIB: u64 = 32 * LOG_SIZE_1_MIB;
/// Sixty-four mebibytes, suitable for high-volume bounded services.
pub const LOG_SIZE_64_MIB: u64 = 64 * LOG_SIZE_1_MIB;
