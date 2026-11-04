cmake_minimum_required(VERSION 3.28)
# Last1 Panel : génération d'images, d'après afmf-linux 1.3.1 (bc56e5b, MIT), voir NOTICE.md.
project(last1-framegen VERSION 1.3.1 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_C_VISIBILITY_PRESET hidden)

option(AFMF_WERROR "Treat warnings as errors" ON)
option(AFMF_SANITIZE "Build with AddressSanitizer + UndefinedBehaviorSanitizer" OFF)

include(GNUInstallDirs)

# The layer only needs the headers: a Vulkan layer must never link the loader itself.
# The test host is an ordinary application and does link it.
find_package(Vulkan REQUIRED)
find_package(Threads REQUIRED)
# VK_EXT_swapchain_maintenance1 and VK_KHR_present_id are the oldest structures the layer carries
# for the application; VK_EXT_present_timing is optional (#ifdef in src/swapchain.c) so the
# distribution packages build against the headers Ubuntu 24.04 and Debian trixie ship.
if(Vulkan_VERSION VERSION_LESS 1.3.250)
    message(FATAL_ERROR "Vulkan headers ${Vulkan_VERSION} are too old; 1.3.250 or newer is needed")
endif()

set(AFMF_WARNINGS
    -Wall -Wextra
    -Wshadow -Wconversion -Wsign-conversion -Wdouble-promotion
    -Wformat=2 -Werror=format-security
    -Wcast-qual -Wcast-align -Wpointer-arith -Wwrite-strings
    -Wstrict-prototypes -Wold-style-definition -Wmissing-prototypes -Wmissing-declarations
    -Wvla -Walloca -Wstack-protector
    -Wnull-dereference -Wimplicit-fallthrough
    -Werror=implicit-function-declaration -Werror=incompatible-pointer-types -Werror=int-conversion
    $<$<BOOL:${AFMF_WERROR}>:-Werror>
)
set(AFMF_SANITIZE_FLAGS -fsanitize=address,undefined -fno-omit-frame-pointer -fno-sanitize-recover=all)

# ---- shaders: GLSL -> SPIR-V -> one embedded header --------------------------------------------

find_program(AFMF_GLSLANG NAMES glslangValidator glslang)
if(NOT AFMF_GLSLANG)
    message(FATAL_ERROR "glslang not found; it compiles the layer's shaders (Fedora: dnf install glslang)")
endif()

# Optional: every compiled module is also checked with spirv-val when SPIRV-Tools is installed.
find_program(AFMF_SPIRV_VAL spirv-val)
if(AFMF_SPIRV_VAL)
    set(AFMF_VALIDATE "${AFMF_SPIRV_VAL}" --target-env vulkan1.1)
else()
    set(AFMF_VALIDATE "${CMAKE_COMMAND}" -E echo "spirv-val not installed, skipping")
endif()
set(AFMF_SPV_DIR "${CMAKE_BINARY_DIR}/shaders")
set(AFMF_FFX_DIR "${CMAKE_SOURCE_DIR}/shaders/fidelityfx")
file(GLOB_RECURSE AFMF_FFX_HEADERS "${AFMF_FFX_DIR}/*.h")
set(AFMF_SPV_FILES)

# afmf_glsl(<output name> <source> [defines...]): compiles one compute shader to ${AFMF_SPV_DIR}/<name>.spv
function(afmf_glsl name source)
    set(spv "${AFMF_SPV_DIR}/${name}.spv")
    add_custom_command(
        OUTPUT "${spv}"
        COMMAND "${CMAKE_COMMAND}" -E make_directory "${AFMF_SPV_DIR}"
        # Vulkan 1.1 / SPIR-V 1.3: the lowest version with subgroup ops, so the layer works
        # inside any application that asked for 1.1 or newer (see api_version in framegen.c).
        COMMAND "${AFMF_GLSLANG}" -V --target-env vulkan1.1 -S comp ${ARGN}
                "-I${AFMF_FFX_DIR}" -o "${spv}" "${source}"
        COMMAND ${AFMF_VALIDATE} "${spv}"
        DEPENDS "${source}" ${AFMF_FFX_HEADERS}
        COMMENT "glslang ${name}"
        VERBATIM
    )
    set(AFMF_SPV_FILES ${AFMF_SPV_FILES} "${spv}" PARENT_SCOPE)
endfunction()

# FidelityFX Optical Flow passes, SDR permutation, exactly as the SDK's Vulkan backend built them.
set(AFMF_FFX_DEFINES -DFFX_GPU=1 -DFFX_GLSL=1 -DFFX_OPTICALFLOW_OPTION_HDR_COLOR_INPUT=0)
foreach(pass
        ffx_opticalflow_prepare_luma_pass
        ffx_opticalflow_compute_luminance_pyramid_pass
        ffx_opticalflow_generate_scd_histogram_pass
        ffx_opticalflow_compute_scd_divergence_pass
        ffx_opticalflow_compute_optical_flow_advanced_pass_v5
        ffx_opticalflow_filter_optical_flow_pass_v5
        ffx_opticalflow_scale_optical_flow_advanced_pass_v5)
    afmf_glsl(${pass} "${AFMF_FFX_DIR}/passes/${pass}.glsl" ${AFMF_FFX_DEFINES})
endforeach()
# The block search once more with the SAD on packed 16-bit pairs, for devices with shaderInt16.
afmf_glsl(ffx_opticalflow_compute_optical_flow_advanced_pass_v5_int16
    "${AFMF_FFX_DIR}/passes/ffx_opticalflow_compute_optical_flow_advanced_pass_v5.glsl"
    ${AFMF_FFX_DEFINES} -DAFMF_SAD_INT16=1)

# The interpolator, once per output format it can write (see src/framegen_format.c).
set(AFMF_INTERP "${CMAKE_SOURCE_DIR}/shaders/afmf_interpolate.comp")
afmf_glsl(afmf_interpolate_rgba8 "${AFMF_INTERP}" -DAFMF_OUT_FORMAT=rgba8 -DAFMF_SWAP_RB=0)
afmf_glsl(afmf_interpolate_rgba8_bgra "${AFMF_INTERP}" -DAFMF_OUT_FORMAT=rgba8 -DAFMF_SWAP_RB=1)
afmf_glsl(afmf_interpolate_rgb10a2 "${AFMF_INTERP}" -DAFMF_OUT_FORMAT=rgb10_a2 -DAFMF_SWAP_RB=0)
afmf_glsl(afmf_interpolate_rgba16f "${AFMF_INTERP}" -DAFMF_OUT_FORMAT=rgba16f -DAFMF_SWAP_RB=0)
afmf_glsl(afmf_interpolate_noformat "${AFMF_INTERP}" -DAFMF_OUT_FORMAT_NONE=1 -DAFMF_SWAP_RB=0)

# The fused ingest (colour ring + luma in one read of the game's frame), once per ring format.
set(AFMF_INGEST "${CMAKE_SOURCE_DIR}/shaders/afmf_ingest.comp")
afmf_glsl(afmf_ingest_rgba8 "${AFMF_INGEST}" -DAFMF_OUT_FORMAT=rgba8)
afmf_glsl(afmf_ingest_rgb10a2 "${AFMF_INGEST}" -DAFMF_OUT_FORMAT=rgb10_a2)
afmf_glsl(afmf_ingest_rgba16f "${AFMF_INGEST}" -DAFMF_OUT_FORMAT=rgba16f)
afmf_glsl(afmf_ingest_noformat "${AFMF_INGEST}" -DAFMF_OUT_FORMAT_NONE=1)

# The half-resolution downsample for performance mode: 8-bit, or half floats for scRGB.
set(AFMF_DOWNSAMPLE "${CMAKE_SOURCE_DIR}/shaders/afmf_downsample.comp")
afmf_glsl(afmf_downsample_rgba8 "${AFMF_DOWNSAMPLE}" -DAFMF_OUT_FORMAT=rgba8)
afmf_glsl(afmf_downsample_rgba16f "${AFMF_DOWNSAMPLE}" -DAFMF_OUT_FORMAT=rgba16f)

set(AFMF_SPIRV_HEADER "${AFMF_SPV_DIR}/afmf_spirv.h")
list(JOIN AFMF_SPV_FILES "|" AFMF_SPV_JOINED)
add_custom_command(
    OUTPUT "${AFMF_SPIRV_HEADER}"
    COMMAND "${CMAKE_COMMAND}" "-DOUTPUT=${AFMF_SPIRV_HEADER}" "-DINPUTS=${AFMF_SPV_JOINED}"
            -P "${CMAKE_SOURCE_DIR}/cmake/embed_spirv.cmake"
    DEPENDS ${AFMF_SPV_FILES} "${CMAKE_SOURCE_DIR}/cmake/embed_spirv.cmake"
    COMMENT "Embedding SPIR-V"
    VERBATIM
)

# ---- the layer: libafmf-linux.so + afmf-linux.json ----------------------------------------------

add_library(afmf-linux SHARED
    src/config.c
    src/framegen.c
    src/framegen_descriptors.c
    src/framegen_dump.c
    src/framegen_format.c
    src/framegen_passes.c
    src/framegen_pipelines.c
    src/framegen_profiler.c
    src/framegen_record.c
    src/framegen_record_frame.c
    src/framegen_resources.c
    src/layer.c
    src/layer_device.c
    src/layer_device_fns.c
    src/layer_device_probe.c
    src/layer_device_queues.c
    src/layer_instance.c
    src/layer_queue.c
    src/layer_registry.c
    src/log.c
    src/swapchain.c
    src/swapchain_cadence.c
    src/swapchain_chain.c
    src/swapchain_create.c
    src/swapchain_destroy.c
    src/swapchain_present_chain.c
    src/swapchain_presenter.c
    src/swapchain_record.c
    src/swapchain_resources.c
    src/swapchain_threads.c
    src/swapchain_worker.c
    "${AFMF_SPIRV_HEADER}"
)
target_include_directories(afmf-linux PRIVATE src "${AFMF_SPV_DIR}")
target_link_libraries(afmf-linux PRIVATE Vulkan::Headers Threads::Threads)
target_compile_definitions(afmf-linux PRIVATE _POSIX_C_SOURCE=200809L)
target_compile_options(afmf-linux PRIVATE ${AFMF_WARNINGS}
    -fstack-protector-strong -fstack-clash-protection -fcf-protection $<$<CONFIG:Release>:-D_FORTIFY_SOURCE=3>)
target_link_options(afmf-linux PRIVATE -Wl,-z,relro,-z,now -Wl,-z,noexecstack)
set_target_properties(afmf-linux PROPERTIES LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/layer"
    OUTPUT_NAME VkLayer_last1_framegen)

# Manifeste : la bibliothèque à côté de lui (Last1 Panel l'installe avec son chemin complet).
set(LAST1_FG_LIBRARY_PATH "./libVkLayer_last1_framegen.so" CACHE STRING "library_path du manifeste")
configure_file(layer/last1-framegen.json.in "${CMAKE_BINARY_DIR}/layer/VkLayer_last1_framegen.json" @ONLY)

install(TARGETS afmf-linux LIBRARY DESTINATION "${CMAKE_INSTALL_LIBDIR}")
install(FILES "${CMAKE_BINARY_DIR}/layer/VkLayer_last1_framegen.json"
        DESTINATION "${CMAKE_INSTALL_DATADIR}/vulkan/explicit_layer.d")

if(AFMF_SANITIZE)
    foreach(target afmf-linux)
        target_compile_options(${target} PRIVATE ${AFMF_SANITIZE_FLAGS})
        target_link_options(${target} PRIVATE ${AFMF_SANITIZE_FLAGS})
    endforeach()
endif()
