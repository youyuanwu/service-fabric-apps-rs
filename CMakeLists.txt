cmake_minimum_required(VERSION 3.27)

project(sf-reliable-collection-rs LANGUAGES CXX)

if(WIN32)
message(STATUS "fetching fabric_metadata")
include(FetchContent)
FetchContent_Declare(fabric_metadata
    GIT_REPOSITORY https://github.com/youyuanwu/service-fabric-metadata-fork.git
    GIT_TAG 552f4e38ad2c772419e52e62207c236fc31ffcbb
)
FetchContent_GetProperties(fabric_metadata)
if(NOT fabric_metadata_POPULATED)
    FetchContent_Populate(fabric_metadata)
    # do not add to cmake build since we only need winmd file to generate code.
endif()

include(${fabric_metadata_SOURCE_DIR}/cmake/ImportFabric.cmake)
message(STATUS "Fabric Code Root: ${ServiceFabric_Runtime_BINARY_DIR}")

add_fabric_lib(
  NAME ReliableCollectionRuntime
  OUTDIR ${CMAKE_CURRENT_BINARY_DIR}
  DLLDIR ${ServiceFabric_Runtime_BINARY_DIR}
)

# generate winmd. defer generation task in .metadata csproj.
find_program (
    dotnet_exe
    NAMES dotnet.exe
    REQUIRED
)

set(_out_winmds
  Microsoft.ServiceFabric.ReliableCollectionRuntime.winmd
  Windows.Win32.winmd
)
set(_out_winmd_paths "")
foreach(_out_winmd ${_out_winmds})
list(APPEND _out_winmd_paths ${CMAKE_CURRENT_SOURCE_DIR}/.windows/winmd/${_out_winmd})
endforeach()

add_custom_command(
  OUTPUT ${_out_winmd_paths}
  COMMAND ${dotnet_exe} build 
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/.metadata
  VERBATIM
)

add_custom_target(generate_winmd
  DEPENDS ${_out_winmd_paths}
  ReliableCollectionRuntime # generator needs to scan support lib files
)
add_custom_target(generate_import_lib ALL
  DEPENDS
  ReliableCollectionRuntime
)
endif(WIN32)

find_program (
    cargo_exe
    NAMES cargo.exe cargo
    REQUIRED
)

add_custom_target(generate_rust
  COMMAND ${cargo_exe} run -p tools_api
  COMMAND ${cargo_exe} fmt -p sfrc-c # format the generated crate
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
  DEPENDS generate_winmd
)

add_custom_target(force_clean
  COMMAND ${CMAKE_COMMAND} -E rm -rf crates/libs/c/src/Microsoft
  COMMAND ${CMAKE_COMMAND} -E rm -rf .windows
  COMMAND ${CMAKE_COMMAND} -E rm -rf build/ReliableCollectionRuntime.lib
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(build_rust ALL
    COMMAND ${cargo_exe} build --all
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
)

enable_testing()
add_subdirectory(crates/samples)
