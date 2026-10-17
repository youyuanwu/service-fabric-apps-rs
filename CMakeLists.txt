cmake_minimum_required(VERSION 3.28)

project(service-fabric-apps-rs LANGUAGES NONE)

find_program (
    cargo_exe
    NAMES cargo.exe cargo
    REQUIRED
)

add_custom_target(build_rust ALL
    COMMAND ${cargo_exe} build --all
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
)

add_subdirectory(crates/samples)
