cmake_minimum_required(VERSION 3.21)
project(viewer_qt LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

find_package(Qt6 6.4 REQUIRED COMPONENTS Quick Qml Gui QuickControls2)

qt_add_executable(viewer-qt main.cpp viewport.cpp viewport.h)
qt_add_resources(viewer-qt "qml" PREFIX "/" FILES qml/Main.qml)
target_link_libraries(viewer-qt PRIVATE Qt6::Quick Qt6::Qml Qt6::Gui Qt6::QuickControls2)
