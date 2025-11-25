#!/bin/bash
# scripts/03_build_xkeyboard-config.sh

set -e

# --- 1. 检查环境 ---
if [ -z "${PROJECT_ROOT}" ] || [ -z "${LIB_DIR}" ] || \
   [ -z "${TOOLCHAIN_DIR}" ] || [ -z "${MESON_CONFIG_FILE_TEMPLATE}" ]; then
    echo "错误：请通过 setup_environment.sh 调用此脚本。" >&2
    exit 1
fi

# 源码配置
XKB_URL="http://www.x.org/releases/individual/data/xkeyboard-config/xkeyboard-config-2.44.tar.xz"
XKB_ARCHIVE_NAME="xkeyboard-config-2.44.tar.xz"
XKB_SRC_DIR="${LIB_DIR}/xkeyboard-config"
XKB_BUILD_DIR="${LIB_DIR}/build/xkeyboard-config-${TARGET_ARCH}"
TMP_MESON_CROSS_FILE="${XKB_BUILD_DIR}/meson.cross.ini"

# --- 2. 准备源码 ---
if [ ! -d "${XKB_SRC_DIR}" ]; then
    # 这里放你原来的下载逻辑
    echo "正在下载源码..."
    mkdir -p "${LIB_DIR}" && cd "${LIB_DIR}"
    wget -c -O "${XKB_ARCHIVE_NAME}" "${XKB_URL}"
    tar -xJf "${XKB_ARCHIVE_NAME}"
    mv "xkeyboard-config-2.44" "${XKB_SRC_DIR}"
    rm "${XKB_ARCHIVE_NAME}"
fi

# --- 3. 生成交叉编译配置 ---
echo "正在配置 xkeyboard-config (目标: ${TARGET_ARCH})..."
mkdir -p "${XKB_BUILD_DIR}"
cd "${XKB_BUILD_DIR}"

# 使用 TOOLCHAIN_DIR 替换 /__TOOLCHAIN_ROOT__
sed "s|/__TOOLCHAIN_ROOT__|${TOOLCHAIN_DIR}|g" "${MESON_CONFIG_FILE_TEMPLATE}" > "${TMP_MESON_CROSS_FILE}"


SYSROOT_BASE=$(grep "sys_root =" "${TMP_MESON_CROSS_FILE}" | cut -d "'" -f 2)

if [ -z "${SYSROOT_BASE}" ]; then
    echo "错误: 无法从 ${TMP_MESON_CROSS_FILE} 解析 sys_root 路径。"
    exit 1
fi

echo "  -> 工具链根目录: ${TOOLCHAIN_DIR}"
echo "  -> 自动解析 Sysroot: ${SYSROOT_BASE}"

# --- 4. 编译与安装 ---

if [ -f "build.ninja" ]; then
    meson setup --reconfigure "${XKB_SRC_DIR}" \
        --cross-file "${TMP_MESON_CROSS_FILE}" \
        --prefix="/usr"
else
    meson setup "${XKB_SRC_DIR}" \
        --cross-file "${TMP_MESON_CROSS_FILE}" \
        --prefix="/usr"
fi

echo "正在安装到 Sysroot..."
# 这里使用我们动态解析出来的 SYSROOT_BASE
meson install --destdir "${SYSROOT_BASE}"

echo "✅ xkeyboard-config 安装完成。"