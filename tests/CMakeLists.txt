cmake_minimum_required(VERSION 3.14)
project(deskhop_tests C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Unity test framework
add_library(unity STATIC
    ${CMAKE_CURRENT_SOURCE_DIR}/unity/src/unity.c
)
target_include_directories(unity PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/unity/src
)

# Stub includes for Pico SDK types
set(STUB_INCLUDES ${CMAKE_CURRENT_SOURCE_DIR}/stubs)

# Project source directory
set(SRC_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../src)

enable_testing()

# Placeholder test
add_executable(test_placeholder test_placeholder.c)
target_link_libraries(test_placeholder unity)
add_test(NAME placeholder COMMAND test_placeholder)
