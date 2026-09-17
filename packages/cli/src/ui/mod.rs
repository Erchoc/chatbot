//! 展示层 · 终端渲染与文案
//!
//! 只负责"怎么画到终端上"：颜色、横幅、spinner、方向键选择器、i18n 文案。
//! 不包含任何业务判断；除 `domain` 外，其他层都可以调用。
pub mod art;
pub mod banner;
pub mod i18n;
pub mod select;
pub mod spinner;
pub mod theme;
