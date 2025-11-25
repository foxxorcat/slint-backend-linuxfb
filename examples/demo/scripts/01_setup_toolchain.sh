#!/bin/bash

# 01_setup_toolchain.sh: 下载并准备交叉编译工具链
# 功能：根据 $TARGET_ARCH 环境变量下载对应的工具链。

set -e

# 变量 $TARGET_ARCH, $LIB_DIR, $TOOLCHAIN_DIR
# 均由父脚本 (setup_environment.sh) 导出。

# --- 1. 检查环境变量 ---
if [ -z "${TARGET_ARCH}" ] || [ -z "${LIB_DIR}" ] || [ -z "${TOOLCHAIN_DIR}" ]; then
    echo "错误：此脚本必须由 setup_environment.sh 调用。" >&2
    echo "缺少必要的环境变量。" >&2
    exit 1
fi

# --- 2. 根据 $TARGET_ARCH 配置下载 ---
# (修正：移除了 'local' 关键字，因为它不在函数内部)
TOOLCHAIN_URL=""
TARBALL_NAME="toolchain.tar.gz"
EXTRACT_COMMAND="tar -xzf"
NEEDS_RENAME=false
EXTRACTED_DIR_NAME="" # 提前声明

if [ "${TARGET_ARCH}" = "arm-musleabihf" ]; then
    TOOLCHAIN_URL="https://musl.cc/arm-linux-musleabihf-cross.tgz"
    TARBALL_NAME="toolchain.tgz"
    # musl.cc 的包解压后目录名就是 'arm-linux-musleabihf-cross'

elif [ "${TARGET_ARCH}" = "arm-gnueabihf" ]; then
    TOOLCHAIN_URL="https://developer.arm.com/-/media/Files/downloads/gnu/14.3.rel1/binrel/arm-gnu-toolchain-14.3.rel1-x86_64-arm-none-linux-gnueabihf.tar.xz"
    TARBALL_NAME="arm-gnueabihf.tar.xz"
    EXTRACT_COMMAND="tar -xJf"
    NEEDS_RENAME=true
    EXTRACTED_DIR_NAME="arm-gnu-toolchain-14.3.rel1-x86_64-arm-none-linux-gnueabihf"
else
    echo "错误 (01_setup): 不支持的 TARGET_ARCH: '${TARGET_ARCH}'" >&2
    exit 1
fi

echo "正在检查 [${TARGET_ARCH}] 交叉编译工具链..."
if [ -d "${TOOLCHAIN_DIR}" ]; then
    echo "工具链已存在于 ${TOOLCHAIN_DIR}。跳过下载。"
    exit 0
fi

# --- 3. 下载与解压 ---
echo "未找到工具链。正在从 ${TOOLCHAIN_URL} 下载..."
mkdir -p "${LIB_DIR}"
cd "${LIB_DIR}"

# (修正：移除了 'local' 关键字)
TARBALL_PATH="${LIB_DIR}/${TARBALL_NAME}"

if command -v curl &> /dev/null; then
    curl -L -o "${TARBALL_PATH}" "${TOOLCHAIN_URL}"
elif command -v wget &> /dev/null; then
    wget -O "${TARBALL_PATH}" "${TOOLCHAIN_URL}"
else
    echo "错误：curl 和 wget 都不可用。请安装其中一个。" >&2
    exit 1
fi

echo "正在解压工具链..."

if [ "$NEEDS_RENAME" = true ]; then
    # (修正：移除了 'local' 关键字)
    TEMP_EXTRACT_DIR="${LIB_DIR}/${EXTRACTED_DIR_NAME}"
    if [ -d "${TEMP_EXTRACT_DIR}" ]; then
        rm -rf "${TEMP_EXTRACT_DIR}"
    fi
    
    # 解压 (tar -xJf xxx.tar.xz)
    ${EXTRACT_COMMAND} "${TARBALL_PATH}"
    
    # 检查解压出的目录是否存在
    if [ ! -d "${TEMP_EXTRACT_DIR}" ]; then
        echo "错误：解压失败，未找到预期的目录 ${TEMP_EXTRACT_DIR}" >&2
        rm "${TARBALL_PATH}"
        exit 1
    fi
    
    # 重命名为我们统一的路径
    mv "${TEMP_EXTRACT_DIR}" "${TOOLCHAIN_DIR}"
    echo "工具链已解压并重命名为 ${TOOLCHAIN_DIR}"

else
    # musl.cc 的 tgz 包解压即是 ${TOOLCHAIN_DIR}
    ${EXTRACT_COMMAND} "${TARBALL_PATH}"
fi

rm "${TARBALL_PATH}"
echo "工具链下载并解压成功。"