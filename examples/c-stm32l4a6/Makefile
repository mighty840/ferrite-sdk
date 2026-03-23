# Ferrite SDK — C FFI example for Nucleo-L4A6ZG
#
# Builds a bare-metal STM32L4A6 firmware that uses ferrite-sdk via C bindings.
# Requires: arm-none-eabi-gcc, libferrite_ffi.a (pre-built for thumbv7em-none-eabihf)
#
# Usage:
#   make                  # Build firmware
#   make flash            # Flash via probe-rs
#   make clean            # Remove build artifacts
#   make ffi              # Rebuild FFI static library + header

PREFIX  = arm-none-eabi-
CC      = $(PREFIX)gcc
AS      = $(PREFIX)gcc
LD      = $(PREFIX)gcc
OBJCOPY = $(PREFIX)objcopy
SIZE    = $(PREFIX)size

# Directories
BUILD_DIR  = build
SRC_DIR    = src
INC_DIR    = include
FFI_DIR    = ../../ferrite-ffi

# Target MCU
MCU     = -mcpu=cortex-m4 -mthumb -mfpu=fpv4-sp-d16 -mfloat-abi=hard

# Source files
C_SOURCES  = $(SRC_DIR)/main.c $(SRC_DIR)/system_stm32l4xx.c
ASM_SOURCES = $(SRC_DIR)/startup_stm32l4a6xx.s

# Includes
INCLUDES = -I$(INC_DIR)

# Compiler flags
CFLAGS  = $(MCU) -Wall -Wextra -Os -g
CFLAGS += -ffunction-sections -fdata-sections -fno-common
CFLAGS += -DSTM32L4A6xx
CFLAGS += $(INCLUDES)

# Assembler flags
ASFLAGS = $(MCU) -g

# Linker flags
LDFLAGS  = $(MCU) -T STM32L4A6ZGTx_FLASH.ld
LDFLAGS += -Wl,--gc-sections -Wl,-Map=$(BUILD_DIR)/firmware.map
LDFLAGS += -specs=nano.specs -specs=nosys.specs
LDFLAGS += -lnosys

# FFI static library
FFI_LIB = ../../target/thumbv7em-none-eabihf/release/libferrite_ffi.a
LDFLAGS += -L$(dir $(FFI_LIB))
LIBS    = -lferrite_ffi -lm -lc -lgcc

# Object files
C_OBJECTS   = $(patsubst $(SRC_DIR)/%.c,$(BUILD_DIR)/%.o,$(C_SOURCES))
ASM_OBJECTS = $(patsubst $(SRC_DIR)/%.s,$(BUILD_DIR)/%.o,$(ASM_SOURCES))
OBJECTS     = $(C_OBJECTS) $(ASM_OBJECTS)

# Default target
all: $(BUILD_DIR)/firmware.elf $(BUILD_DIR)/firmware.bin
	$(SIZE) $(BUILD_DIR)/firmware.elf

# Compile C sources
$(BUILD_DIR)/%.o: $(SRC_DIR)/%.c | $(BUILD_DIR)
	$(CC) $(CFLAGS) -c $< -o $@

# Assemble startup
$(BUILD_DIR)/%.o: $(SRC_DIR)/%.s | $(BUILD_DIR)
	$(AS) $(ASFLAGS) -c $< -o $@

# Link
$(BUILD_DIR)/firmware.elf: $(OBJECTS) $(FFI_LIB)
	$(LD) $(LDFLAGS) $(OBJECTS) $(LIBS) -o $@

# Binary for flashing
$(BUILD_DIR)/firmware.bin: $(BUILD_DIR)/firmware.elf
	$(OBJCOPY) -O binary $< $@

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

# Build FFI static library and generate header
ffi:
	cd $(FFI_DIR) && cargo build --release --target thumbv7em-none-eabihf
	cbindgen --config $(FFI_DIR)/cbindgen.toml --crate ferrite-ffi --output $(INC_DIR)/ferrite-sdk.h

# Flash via probe-rs
flash: $(BUILD_DIR)/firmware.elf
	probe-rs run --chip STM32L4A6ZGTx $(BUILD_DIR)/firmware.elf

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all ffi flash clean
