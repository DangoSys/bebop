// BEMU C API 模块
//
// 这个模块导出 C 兼容的 API，供 Spike (C++) 调用
//
// 编译后生成：
// - libbemu.rlib (Rust 库)
// - libbemu.cdylib / libbemu.so (动态库)
// - libbemu.a (静态库)
//
// Spike 需要链接这些库才能调用 BEMU
//
// 注意：C API 函数定义在 spike_interface.rs 中，
// 这个文件只作为模块入口，不重新导出函数以避免符号重复。

// 空模块，C API 函数在 spike_interface.rs 中定义
