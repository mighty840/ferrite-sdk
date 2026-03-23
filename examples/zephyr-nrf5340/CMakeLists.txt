# Ferrite SDK — Zephyr BLE example for nRF5340-DK
#
# Builds a Zephyr application that uses ferrite-sdk via C FFI bindings
# and transmits telemetry chunks over BLE GATT notifications.
#
# Build:
#   west build -b nrf5340dk/nrf5340/cpuapp
#   west flash

cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})

project(ferrite-zephyr-nrf5340 LANGUAGES C)

# Source files
target_sources(app PRIVATE
    src/main.c
)

# Include ferrite SDK C header
target_include_directories(app PRIVATE
    include
)

# Link pre-built ferrite FFI static library (thumbv8m.main-none-eabihf)
target_link_libraries(app PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/lib/libferrite_ffi.a
)
