cmake_minimum_required(VERSION 3.15)
project(pixofold_jpeg_lab LANGUAGES C)

# 上游要求独立构建；这里只链接脚本已构建的静态库，不修改其构建规则。
find_library(MOZJPEG_LIBRARY NAMES jpeg-static jpeg
  PATHS "${MOZJPEG_BUILD}" "${MOZJPEG_BUILD}/Release" NO_DEFAULT_PATH)
if(NOT MOZJPEG_LIBRARY)
  message(FATAL_ERROR "Missing MozJPEG static library; build the engine first")
endif()

add_executable(pixofold-coeffdump coeffdump.c)

# J1a核心工具不包含实验语料命令，不自动分发到桌面。
add_executable(pixofold-jpeg-helper ../../native/jpeg/helper.c)
target_compile_features(pixofold-jpeg-helper PRIVATE c_std_99)
target_include_directories(pixofold-jpeg-helper PRIVATE "${MOZJPEG_SOURCE}" "${MOZJPEG_BUILD}")
target_link_libraries(pixofold-jpeg-helper PRIVATE "${MOZJPEG_LIBRARY}")
if(UNIX)
  target_link_libraries(pixofold-jpeg-helper PRIVATE m)
endif()
if(MSVC)
  target_compile_options(pixofold-jpeg-helper PRIVATE /utf-8 /W4 /WX)
else()
  target_compile_options(pixofold-jpeg-helper PRIVATE -Wall -Wextra -Werror)
endif()
target_compile_features(pixofold-coeffdump PRIVATE c_std_99)
target_include_directories(pixofold-coeffdump PRIVATE
  "${MOZJPEG_SOURCE}" "${MOZJPEG_BUILD}")
target_link_libraries(pixofold-coeffdump PRIVATE "${MOZJPEG_LIBRARY}")
if(UNIX)
  target_link_libraries(pixofold-coeffdump PRIVATE m)
endif()
if(MSVC)
  target_compile_options(pixofold-coeffdump PRIVATE /utf-8 /W4 /WX)
else()
  target_compile_options(pixofold-coeffdump PRIVATE -Wall -Wextra -Werror)
endif()
