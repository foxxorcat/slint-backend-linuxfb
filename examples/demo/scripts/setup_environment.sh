#!/bin/bash

# setup_environment.sh: 用于设置交叉编译环境的主控制脚本。
#
# 用法:
#   ./setup_environment.sh {all|toolchain|lapack|clean}
#
set -e

PROJECT_ROOT=$(cd "$(dirname "$0")/.." && pwd)
SCRIPTS_DIR="${PROJECT_ROOT}/scripts"
LIB_DIR="${PROJECT_ROOT}/lib"
CONFIG_FILE="${SCRIPTS_DIR}/.build_config" # 用于存储配置的文件

# --- 0. 架构选择与持久化 ---
task_select_target_arch() {
    # 优先级 1: 环境变量 (如果用户手动 export TARGET_ARCH=... 运行脚本)
    if [ -n "${TARGET_ARCH}" ]; then
        echo "ℹ️  使用环境变量 TARGET_ARCH: ${TARGET_ARCH}"
        # 更新配置文件以备下次使用
        echo "TARGET_ARCH=${TARGET_ARCH}" > "${CONFIG_FILE}"
        return 0
    fi

    # 优先级 2: 读取配置文件
    if [ -f "${CONFIG_FILE}" ]; then
        source "${CONFIG_FILE}"
        if [ -n "${TARGET_ARCH}" ]; then
            echo "✅ 从配置加载目标架构: ${TARGET_ARCH}"
            return 0
        fi
    fi

    # 优先级 3: 交互式选择
    echo "--- 未检测到配置，请选择目标架构 ---"
    echo "  1) arm-musleabihf (musl.cc)"
    echo "  2) arm-gnueabihf  (Arm Official)"
    
    local ARCH_OPTION
    read -p "请输入选项 [1-2]: " ARCH_OPTION

    case "$ARCH_OPTION" in
        1) TARGET_ARCH="arm-musleabihf" ;;
        2) TARGET_ARCH="arm-gnueabihf" ;;
        *) echo "错误: 无效选项"; exit 1 ;;
    esac

    # 保存配置
    echo "TARGET_ARCH=${TARGET_ARCH}" > "${CONFIG_FILE}"
    echo "已保存配置到 ${CONFIG_FILE}"
}

# --- 1. 动态配置路径 ---
setup_dynamic_paths() {
    # 此时 TARGET_ARCH 一定有值
    
    # 映射工具链目录名
    if [ "${TARGET_ARCH}" = "arm-musleabihf" ]; then
        TOOLCHAIN_DIR_NAME="arm-linux-musleabihf-cross"
    elif [ "${TARGET_ARCH}" = "arm-gnueabihf" ]; then
        TOOLCHAIN_DIR_NAME="arm-gnueabihf-cross"
    fi

    # 导出所有必要的变量供子脚本使用
    export PROJECT_ROOT
    export SCRIPTS_DIR
    export LIB_DIR
    export TARGET_ARCH
    export TOOLCHAIN_DIR="${LIB_DIR}/${TOOLCHAIN_DIR_NAME}"
    export CMAKE_TOOLCHAIN_FILE="${SCRIPTS_DIR}/${TARGET_ARCH}-toolchain.cmake"
    export MESON_CONFIG_FILE_TEMPLATE="${SCRIPTS_DIR}/meson-${TARGET_ARCH}.ini"
    export LAPACK_BUILD_DIR="${LIB_DIR}/build/lapack-${TARGET_ARCH}"
    export STATE_DIR="${LIB_DIR}/.state/${TARGET_ARCH}"

    # 创建状态目录
    mkdir -p "${STATE_DIR}"
}

task_setup_toolchain() {
    "${SCRIPTS_DIR}/01_setup_toolchain.sh"
}

task_build_lapack() {
    if [ ! -d "${TOOLCHAIN_DIR}" ]; then task_setup_toolchain; fi
    "${SCRIPTS_DIR}/02_build_lapack.sh"
}

task_build_xkb() {
    if [ ! -d "${TOOLCHAIN_DIR}" ]; then task_setup_toolchain; fi
    
    echo "▶️  正在运行: 03_build_xkeyboard-config.sh"
    chmod +x "${SCRIPTS_DIR}/03_build_xkeyboard-config.sh"
    "${SCRIPTS_DIR}/03_build_xkeyboard-config.sh"
}

task_clean() {
    echo "🔥 清理构建..."
    rm -rf "${LIB_DIR}/build"
    rm -f "${CONFIG_FILE}"
    echo "配置已重置。"
}

# --- 主入口 ---
main() {
    task_select_target_arch
    setup_dynamic_paths

    if [ -z "$1" ]; then
        echo "用法: $0 {toolchain|lapack|xkb|all|clean}"
        echo "当前目标: ${TARGET_ARCH}"
        exit 1
    fi

    case "$1" in
        toolchain) task_setup_toolchain ;;
        lapack)    task_build_lapack ;;
        xkb)       task_build_xkb ;;
        all)
            task_setup_toolchain
            task_build_lapack
            task_build_xkb
            ;;
        clean)     task_clean ;;
        *) echo "未知命令: $1"; exit 1 ;;
    esac
}

main "$@"