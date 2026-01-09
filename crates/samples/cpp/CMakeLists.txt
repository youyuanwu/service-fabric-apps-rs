# generate winmd
find_program (
  dotnet_exe
  NAMES dotnet.exe
  REQUIRED
)

set(winmd_file ${CMAKE_CURRENT_SOURCE_DIR}/.windows/winmd/Microsoft.ServiceFabric.FabricStrings.winmd)

add_custom_command(
  OUTPUT ${winmd_file}
  COMMAND ${dotnet_exe} build
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/.metadata
  VERBATIM
)

add_custom_target(generate_fabric_strings_winmd
  DEPENDS ${winmd_file}
)

add_custom_target(generate_fabric_strings_winmd_clean
  # COMMAND ${CMAKE_COMMAND} -E rm -f ${winmd_file}
  COMMAND ${dotnet_exe} clean
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/.metadata
)

add_custom_target(generate_fabric_strings_rust
    COMMAND ${cargo_exe} run -p cpp_tools_api --bin cpp_tools_api
    COMMAND ${cargo_exe} fmt -p cpp # format the generated crate
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
    DEPENDS generate_fabric_strings_winmd
)

# generate cpp header
find_program (
    MIDL_exe
    NAMES midl.exe
    REQUIRED
)
set(ids_src ${CMAKE_CURRENT_SOURCE_DIR}/FabricStrings.idl)
set(output_code ${CMAKE_CURRENT_BINARY_DIR}/FabricStrings.h ${CMAKE_CURRENT_BINARY_DIR}/FabricStrings_i.c)
add_custom_command(
  DEPENDS ${ids_src}
  OUTPUT ${output_code}
  COMMAND ${MIDL_exe} /no_settings_comment /utf8 /sal /sal_local /I ${fabric_metadata_SOURCE_DIR}/idl 
    ${ids_src} /out ${CMAKE_CURRENT_BINARY_DIR}
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
  VERBATIM
)

set(cpp_lib ${CMAKE_SOURCE_DIR}/target/debug/cpp.dll.lib)

set (CMAKE_CXX_STANDARD 20)
set(exe_name cpp_main)
add_executable(${exe_name}
  ${output_code}
  main.cpp
)

# see list cargo rustc -p cpp --lib -q -- --print=native-static-libs
target_link_libraries(${exe_name}
  PUBLIC ${cpp_lib}
)

# make sure rust static lib is generated
add_dependencies(${exe_name} build_rust)

target_include_directories(${exe_name}
  PUBLIC ${CMAKE_CURRENT_BINARY_DIR}
  PUBLIC ${fabric_metadata_SOURCE_DIR}/src/fabric/include
)

# copy rust dll to cpp out dir
add_custom_command(TARGET ${exe_name} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
    "${CMAKE_SOURCE_DIR}/target/debug/cpp.dll"
    $<TARGET_FILE_DIR:${exe_name}>
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
    "${CMAKE_SOURCE_DIR}/target/debug/cpp.pdb"
    $<TARGET_FILE_DIR:${exe_name}>
)

add_test(NAME ${exe_name} COMMAND ${exe_name} WORKING_DIRECTORY ${CMAKE_BINARY_DIR})