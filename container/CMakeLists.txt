add_custom_target(
  build_image
  COMMAND docker build -t sf-win:latest .
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(
  launch_onebox
  COMMAND docker run --rm -p 19080:19080 sf-win:latest
  WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)