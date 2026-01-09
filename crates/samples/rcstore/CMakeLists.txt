# copy files to build folder to form a code package.

add_custom_target(build_rust_sample_rcstore ALL
  DEPENDS build_rust
)

set(_pkg_root ${CMAKE_BINARY_DIR}/rcstore_root)
set(_pkg_src  ${CMAKE_SOURCE_DIR}/crates/samples/rcstore)
if(WIN32)
  set(_pkg_exe  ${CMAKE_SOURCE_DIR}/target/debug/rcstore.exe)
else()
  set(_pkg_exe  ${CMAKE_SOURCE_DIR}/target/debug/rcstore)
endif(WIN32)
add_custom_command(TARGET build_rust_sample_rcstore POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory ${_pkg_root}
    COMMAND ${CMAKE_COMMAND} 
    -E copy_if_different ${_pkg_src}/manifests/RcStoreServicePackage/ServiceManifest.xml ${_pkg_root}/RcStoreServicePackage/ServiceManifest.xml
    COMMAND ${CMAKE_COMMAND} 
    -E copy_if_different ${_pkg_src}/manifests/ApplicationManifest.xml ${_pkg_root}/ApplicationManifest.xml
    COMMAND ${CMAKE_COMMAND} 
    -E copy_if_different ${_pkg_exe} ${_pkg_root}/RcStoreServicePackage/Code/rcstore.exe
    # COMMAND ${CMAKE_COMMAND} 
    # -E copy_if_different ${CMAKE_SOURCE_DIR}/target/debug/kvstore.pdb ${_pkg_root}/RcStoreServicePackage/Code/kvstore.pdb
)

if(WIN32)
  # need to generate import lib
  add_dependencies(
    build_rust
    ReliableCollectionRuntime
  )
endif(WIN32)