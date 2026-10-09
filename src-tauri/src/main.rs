// 发布构建不要在 Windows 上弹出一个控制台窗口（调试时留着，方便看日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! `peon-hall` 的入口：只做一件事 —— 把控制权交给 `peon_hall_lib::run()`。

fn main() {
    peon_hall_lib::run()
}
