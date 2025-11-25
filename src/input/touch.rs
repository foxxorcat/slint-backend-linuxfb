//! 触摸手势处理模块
//!
//! 本模块负责处理来自 `evdev` 的触摸屏事件，包括：
//! - 多点触控协议解析 (支持 Protocol A 和 Protocol B)。
//! - 坐标映射与校准。
//! - 手势识别：单指点击、单指拖拽、长按右键、双指滚动。

use evdev::{AbsInfo, AbsoluteAxisCode};
use i_slint_core::api::PhysicalPosition;
use i_slint_core::platform::{PointerEventButton, WindowEvent};
use std::time::{Duration, Instant};

/// 像素级去抖动阈值：只有移动距离超过此值才视为有效移动，防止静止时的微小抖动。
const JITTER_THRESHOLD: i32 = 2;

/// 点击操作允许的最大漂移距离（像素）：按下和抬起位置距离超过此值则视为拖拽而非点击。
const TAP_DRIFT_THRESHOLD: i32 = 20;

/// 长按触发右键的时间阈值。
const LONG_PRESS_DURATION: Duration = Duration::from_millis(600);

/// 滚动速度缩放因子：将触摸移动距离转换为滚动距离的倍率。
const SCROLL_SCALE: f32 = 2.0;

/// 支持的最大硬件触控点数量 (Slot)。虽然通常只需要处理前两个点，但保留余量以防万一。
const MAX_SLOTS: usize = 10;

/// 单个触控点 (Slot) 的内部状态
#[derive(Debug, Clone, Copy, Default)]
pub struct SlotState {
    /// 该 Slot 是否处于活跃状态 (手指按下)
    pub active: bool,
    /// 追踪 ID (Tracking ID)，用于区分不同的手指序列
    pub id: i32,
    /// 原始 X 坐标
    pub x: i32,
    /// 原始 Y 坐标
    pub y: i32,
}

/// 手势识别状态机模式
#[derive(Debug, Clone, Copy, PartialEq)]
enum GestureMode {
    /// 无手势 / 初始状态
    None,
    /// 单指操作：模拟鼠标左键移动或点击
    Pointer,
    /// 右键拖拽：单指长按后触发，模拟鼠标右键
    RightDrag,
    /// 滚动：双指移动触发，模拟鼠标滚轮
    Scroll,
    /// 等待释放：手势结束或无效状态，等待所有手指抬起
    WaitRelease,
}

/// 触摸屏全局状态管理器
pub struct TouchState {
    /// 所有触控点的状态数组
    pub slots: [SlotState; MAX_SLOTS],
    /// 当前正在处理的 Slot 索引 (用于 Protocol B)
    pub current_slot: usize,

    // --- 手势相关状态 ---
    gesture_mode: GestureMode,
    /// 手势开始时间 (用于长按检测)
    gesture_start_time: Option<Instant>,
    /// 手势开始时的重心坐标 (用于检测点击漂移)
    initial_centroid: Option<PhysicalPosition>,
    /// 上一帧的重心坐标 (用于计算相对移动 delta)
    last_centroid: Option<PhysicalPosition>,
    /// 上一次向 Slint 报告的指针位置 (用于去抖动)
    last_reported_pos: Option<PhysicalPosition>,

    /// 当前手势周期内检测到的最大手指数量
    max_fingers_down: usize,
    /// 标记长按是否已失效 (例如已经发生了移动)
    long_press_invalidated: bool,
}

impl TouchState {
    pub fn new() -> Self {
        Self {
            slots: [SlotState::default(); MAX_SLOTS],
            current_slot: 0,
            gesture_mode: GestureMode::None,
            gesture_start_time: None,
            initial_centroid: None,
            last_centroid: None,
            last_reported_pos: None,
            max_fingers_down: 0,
            long_press_invalidated: false,
        }
    }

    /// 处理 evdev 的绝对坐标 (ABS) 事件
    ///
    /// 支持 Multi-touch Protocol A (无状态) 和 Protocol B (有状态，基于 Slot)。
    /// 同时兼容部分仅报告 ABS_X/ABS_Y 的单点触摸设备。
    pub fn process_axis(&mut self, code: AbsoluteAxisCode, value: i32, is_protocol_b: bool) {
        match code {
            // --- MT Protocol B: Slot 切换 ---
            AbsoluteAxisCode::ABS_MT_SLOT => {
                if (value as usize) < MAX_SLOTS {
                    self.current_slot = value as usize;
                }
            }
            // --- MT Protocol B: 追踪 ID ---
            AbsoluteAxisCode::ABS_MT_TRACKING_ID => {
                if self.current_slot < MAX_SLOTS {
                    if value == -1 {
                        // ID 为 -1 表示手指抬起
                        self.slots[self.current_slot].active = false;
                    } else {
                        // 新 ID 表示手指按下或更新
                        self.slots[self.current_slot].active = true;
                        self.slots[self.current_slot].id = value;
                    }
                }
            }
            // --- MT 坐标数据 ---
            AbsoluteAxisCode::ABS_MT_POSITION_X => {
                if self.current_slot < MAX_SLOTS {
                    self.slots[self.current_slot].x = value;
                    // Protocol A 兼容：如果不是 B 协议，收到坐标即视为活跃
                    if !is_protocol_b && !self.slots[self.current_slot].active {
                        self.slots[self.current_slot].active = true;
                    }
                }
            }
            AbsoluteAxisCode::ABS_MT_POSITION_Y => {
                if self.current_slot < MAX_SLOTS {
                    self.slots[self.current_slot].y = value;
                    if !is_protocol_b && !self.slots[self.current_slot].active {
                        self.slots[self.current_slot].active = true;
                    }
                }
            }
            // --- 单点触摸兼容 (Legacy) ---
            // 某些驱动在发送 MT 事件的同时也会发送传统的 ABS_X/Y，
            // 或者对于不支持 MT 的老设备，只发送这两个事件。
            // 我们将其映射到 Slot 0 以保证兼容性。
            AbsoluteAxisCode::ABS_X => {
                self.slots[0].x = value;
                if !self.slots[0].active {
                    self.slots[0].active = true;
                }
            }
            AbsoluteAxisCode::ABS_Y => {
                self.slots[0].y = value;
                if !self.slots[0].active {
                    self.slots[0].active = true;
                }
            }
            _ => {}
        }
    }

    /// 处理 Protocol A 的 SYN_MT_REPORT 同步信号
    ///
    /// 在 Protocol A 中，每个触点数据包以 SYN_MT_REPORT 结束。
    /// 我们需要手动递增 Slot 索引来为下一个触点做准备。
    pub fn sync_mt_report(&mut self) {
        self.current_slot += 1;
        if self.current_slot >= MAX_SLOTS {
            self.current_slot = MAX_SLOTS - 1;
        }
    }

    /// 处理 Protocol A 的帧结束
    ///
    /// Protocol A 不显式发送“抬起”事件，而是通过不再报告该触点来表示。
    /// 因此在帧结束时，未被更新的后续 Slot 应被标记为非活跃。
    pub fn finish_frame_protocol_a(&mut self) {
        for i in self.current_slot..MAX_SLOTS {
            self.slots[i].active = false;
        }
        self.current_slot = 0;
    }
}

/// 分析触摸数据并生成 Slint 事件
///
/// 该函数在每帧同步 (SYN_REPORT) 时调用。它计算所有活跃触点的几何重心，
/// 并根据手指数量和持续时间维护手势状态机。
pub fn analyze_touch_gesture(
    state: &mut TouchState,
    pointer_pos: &mut PhysicalPosition,
    is_left_pressed: &mut bool,
    screen_width: u32,
    screen_height: u32,
    abs_x: &Option<AbsInfo>,
    abs_y: &Option<AbsInfo>,
) -> Option<Vec<WindowEvent>> {
    // 1. 统计活跃手指
    let mut active_slots = Vec::new();
    for (i, slot) in state.slots.iter().enumerate() {
        if slot.active {
            active_slots.push(i);
        }
    }
    let finger_count = active_slots.len();
    let mut events = Vec::new();

    // 坐标映射闭包：将原始设备坐标映射到屏幕像素坐标
    let map_coord = |val: i32, info: &Option<AbsInfo>, screen_max: u32| -> i32 {
        if let Some(info) = info {
            let range = (info.maximum() - info.minimum()) as f32;
            if range > 0.0 {
                return ((val - info.minimum()) as f32 / range * screen_max as f32).round() as i32;
            }
        }
        // 兜底：如果没有获取到 abs info，直接返回原始值
        val
    };

    // 2. 计算重心 (Centroid)
    // 多指操作时，我们使用所有手指的中心点作为光标位置
    let (cx, cy) = if finger_count > 0 {
        let (sum_x, sum_y) = active_slots.iter().fold((0, 0), |acc, &idx| {
            (acc.0 + state.slots[idx].x, acc.1 + state.slots[idx].y)
        });
        (sum_x / finger_count as i32, sum_y / finger_count as i32)
    } else {
        (0, 0)
    };

    let screen_cx = map_coord(cx, abs_x, screen_width);
    let screen_cy = map_coord(cy, abs_y, screen_height);
    let current_centroid = PhysicalPosition::new(screen_cx, screen_cy);

    // 3. 初始化新手势
    if finger_count > 0 && state.gesture_start_time.is_none() {
        state.gesture_start_time = Some(Instant::now());
        state.initial_centroid = Some(current_centroid);
        state.max_fingers_down = finger_count;
        state.long_press_invalidated = false;
    }
    if finger_count > 0 {
        state.max_fingers_down = state.max_fingers_down.max(finger_count);
    }

    // 4. 状态机分支处理
    // 只要检测到两指或更多，优先进入滚动模式，提高误触容忍度
    if finger_count >= 2 {
        // --- 双指 (及以上) 滚动模式 ---

        // 状态清理：如果之前处于按压状态，先释放
        if *is_left_pressed {
            *is_left_pressed = false;
            events.push(WindowEvent::PointerReleased {
                position: pointer_pos.to_logical(1.0),
                button: PointerEventButton::Left,
            });
        }
        if state.gesture_mode == GestureMode::RightDrag {
            events.push(WindowEvent::PointerReleased {
                position: pointer_pos.to_logical(1.0),
                button: PointerEventButton::Right,
            });
        }

        let just_entered = state.gesture_mode != GestureMode::Scroll;
        state.gesture_mode = GestureMode::Scroll;

        // 滚动时更新指针位置到重心，保持视觉连贯性
        *pointer_pos = current_centroid;

        if just_entered {
            state.last_centroid = Some(current_centroid);
        } else {
            if let Some(last) = state.last_centroid {
                let dx = (current_centroid.x - last.x) as f32;
                let dy = (current_centroid.y - last.y) as f32;

                // 滚动去抖：只有移动量超过阈值才生成事件
                if dx.abs() > 0.5 || dy.abs() > 0.5 {
                    events.push(WindowEvent::PointerScrolled {
                        position: pointer_pos.to_logical(1.0),
                        delta_x: dx * SCROLL_SCALE,
                        delta_y: dy * SCROLL_SCALE,
                    });
                }
            }
            state.last_centroid = Some(current_centroid);
        }
    } else {
        match finger_count {
            0 => {
                // --- 0 指：释放/结束 ---
                if state.gesture_mode == GestureMode::RightDrag {
                    events.push(WindowEvent::PointerReleased {
                        position: pointer_pos.to_logical(1.0),
                        button: PointerEventButton::Right,
                    });
                } else if *is_left_pressed {
                    *is_left_pressed = false;
                    events.push(WindowEvent::PointerReleased {
                        position: pointer_pos.to_logical(1.0),
                        button: PointerEventButton::Left,
                    });
                }

                // 重置所有状态
                state.gesture_mode = GestureMode::None;
                state.gesture_start_time = None;
                state.last_centroid = None;
                state.last_reported_pos = None;
                state.initial_centroid = None;
                state.max_fingers_down = 0;
            }
            1 => {
                // --- 1 指：点击 / 拖拽 / 长按 ---

                // 如果刚从滚动模式退出，进入等待释放状态，防止立刻触发点击
                if state.gesture_mode == GestureMode::Scroll {
                    state.gesture_mode = GestureMode::WaitRelease;
                }

                if state.gesture_mode == GestureMode::WaitRelease {
                    // 忽略输入，直到手指完全抬起
                } else if state.gesture_mode == GestureMode::RightDrag {
                    // 保持右键拖拽状态
                    let moved = match state.last_reported_pos {
                        Some(last) => {
                            (current_centroid.x - last.x).abs() > JITTER_THRESHOLD
                                || (current_centroid.y - last.y).abs() > JITTER_THRESHOLD
                        }
                        None => true,
                    };
                    if moved {
                        *pointer_pos = current_centroid;
                        state.last_reported_pos = Some(current_centroid);
                        events.push(WindowEvent::PointerMoved {
                            position: pointer_pos.to_logical(1.0),
                        });
                    }
                } else {
                    // 默认模式：左键逻辑
                    state.gesture_mode = GestureMode::Pointer;

                    // 检查点击漂移：如果位移过大，则该次触摸不能作为长按触发器
                    if !state.long_press_invalidated {
                        if let Some(start) = state.initial_centroid {
                            let dx = (start.x - current_centroid.x).abs();
                            let dy = (start.y - current_centroid.y).abs();
                            if dx > TAP_DRIFT_THRESHOLD || dy > TAP_DRIFT_THRESHOLD {
                                state.long_press_invalidated = true;
                            }
                        }
                    }

                    // 移动去抖
                    let moved = match state.last_reported_pos {
                        Some(last) => {
                            (current_centroid.x - last.x).abs() > JITTER_THRESHOLD
                                || (current_centroid.y - last.y).abs() > JITTER_THRESHOLD
                        }
                        None => true,
                    };

                    if moved {
                        *pointer_pos = current_centroid;
                        state.last_reported_pos = Some(current_centroid);
                        events.push(WindowEvent::PointerMoved {
                            position: pointer_pos.to_logical(1.0),
                        });
                    }

                    // 确保左键按下
                    if !*is_left_pressed {
                        *is_left_pressed = true;
                        events.push(WindowEvent::PointerPressed {
                            position: pointer_pos.to_logical(1.0),
                            button: PointerEventButton::Left,
                        });
                    }

                    // 长按检测逻辑 (触发右键)
                    // 条件：手指未抬起 + 没有发生大幅位移 + 时间超过阈值
                    if let Some(start_time) = state.gesture_start_time {
                        if !state.long_press_invalidated
                            && start_time.elapsed() > LONG_PRESS_DURATION
                        {
                            state.gesture_mode = GestureMode::RightDrag;
                            // 状态切换：释放左键 -> 按下右键
                            *is_left_pressed = false;
                            events.push(WindowEvent::PointerReleased {
                                position: pointer_pos.to_logical(1.0),
                                button: PointerEventButton::Left,
                            });
                            events.push(WindowEvent::PointerPressed {
                                position: pointer_pos.to_logical(1.0),
                                button: PointerEventButton::Right,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Some(events)
}
