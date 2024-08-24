set(exe_name cpp_main)
add_executable(${exe_name}
  main.cpp
)

add_test(NAME ${exe_name} COMMAND ${exe_name} WORKING_DIRECTORY ${CMAKE_BINARY_DIR})