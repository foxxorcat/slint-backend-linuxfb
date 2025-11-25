#!/bin/bash

# 02_build_lapack.sh: 下载、编译并安装 LAPACK
# 功能：使用 $CMAKE_TOOLCHAIN_FILE 环境变量指定的工具链来构建 LAPACK。

set -e

# --- 1. 检查环境变量 (由 setup_environment.sh 导出) ---
if [ -z "${PROJECT_ROOT}" ] || [ -z "${SCRIPTS_DIR}" ] || [ -z "${LIB_DIR}" ] || \
   [ -z "${TOOLCHAIN_DIR}" ] || [ -z "${CMAKE_TOOLCHAIN_FILE}" ] || \
   [ -z "${LAPACK_BUILD_DIR}" ]; then
    echo "错误：此脚本必须由 setup_environment.sh 调用。" >&2
    echo "缺少必要的环境变量。" >&2
    exit 1
fi

# --- 2. 配置路径 (现在从环境变量读取) ---
LAPACK_SRC_DIR="${LIB_DIR}/lapack"
LAPACK_GIT_URL="https://github.com/Reference-LAPACK/lapack.git"
LAPACK_VERSION_TAG="v3.12.1" # 保持版本一致

# --- 3. 检查依赖 (现在使用动态路径) ---
echo "正在检查 LAPACK 构建依赖 (目标: ${TARGET_ARCH:-unknown})..."
if [ ! -d "${TOOLCHAIN_DIR}" ]; then
    echo "错误：在 ${TOOLCHAIN_DIR} 未找到工具链目录" >&2
    echo "请先运行 'setup_environment.sh toolchain'。" >&2
    exit 1
fi
if [ ! -f "${CMAKE_TOOLCHAIN_FILE}" ]; then
    echo "错误：在 ${CMAKE_TOOLCHAIN_FILE} 未找到 CMake 工具链文件" >&2
    exit 1
fi

# --- 4. 下载 LAPACK 源码 (不变) ---
echo "正在检查 LAPACK 源码..."
if [ -d "${LAPACK_SRC_DIR}" ]; then
    echo "LAPACK 源码已存在。跳过下载。"
else
    echo "正在从 ${LAPACK_GIT_URL} 克隆 LAPACK (版本 ${LAPACK_VERSION_TAG})..."
    git clone --depth 1 --branch "${LAPACK_VERSION_TAG}" "${LAPACK_GIT_URL}" "${LAPACK_SRC_DIR}"
    echo "LAPACK 源码克隆成功。"
fi

# --- 5. 编译与安装 (使用动态路径) ---
echo "正在为 [${TARGET_ARCH}] 目标配置 LAPACK..."
# LAPACK_BUILD_DIR 现在是动态的，例如: lib/build/lapack-arm-gnueabihf
mkdir -p "${LAPACK_BUILD_DIR}"
cd "${LAPACK_BUILD_DIR}"

echo "使用 CMake Toolchain: ${CMAKE_TOOLCHAIN_FILE}"

cmake "${LAPACK_SRC_DIR}" \
    -DCMAKE_TOOLCHAIN_FILE="${CMAKE_TOOLCHAIN_FILE}" \
    -DCBLAS=ON \
    -DLAPACKE=ON \
    -DLAPACKE_WITH_TMG=ON \
    -DBUILD_SINGLE=ON \
    -DBUILD_DOUBLE=ON \
    -DBUILD_COMPLEX=ON \
    -DBUILD_COMPLEX16=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_BUILD_TYPE=Release

echo "正在构建 LAPACK [${TARGET_ARCH}]... (这可能需要一些时间)"
make -j$(nproc)

# CMAKE_INSTALL_PREFIX 是在 .cmake 文件中定义的
# 它会自动安装到对应工具链的 sysroot/usr 目录下
echo "正在安装 LAPACK 库到 [${TARGET_ARCH}] 的 sysroot..."
make install

echo "LAPACK [${TARGET_ARCH}] 构建和安装完成。"