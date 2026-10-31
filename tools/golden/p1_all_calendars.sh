#!/bin/sh
# Builds tools/golden/p1_all_calendars.cpp against an unchanged QuantLib v1.42 source tree and
# writes the golden calendar files.
#
# Usage (from the Riemann root): bash tools/golden/p1_all_calendars.sh <quantlib-v1.42> <boost-include> <build-dir>
#   git clone --depth 1 --branch v1.42 https://github.com/lballabio/QuantLib.git <quantlib-v1.42>
# Used on Windows with g++ 14.3.0 and the Boost headers of Rtools 4.5, on the PATH:
#   /c/rtools45/x86_64-w64-mingw32.static.posix/{bin,include}
# Takes a few minutes the first time; existing object files are reused.
set -e
QL=$1
BOOST=$2
BUILD=$3
ROOT=$(pwd)
mkdir -p "$BUILD"
cd "$BUILD"
for f in "$QL"/ql/time/calendars/*.cpp "$QL"/ql/time/calendar.cpp "$QL"/ql/time/date.cpp \
         "$QL"/ql/time/weekday.cpp "$QL"/ql/time/period.cpp "$QL"/ql/time/timeunit.cpp \
         "$QL"/ql/errors.cpp "$QL"/ql/utilities/dataformatters.cpp "$QL"/ql/utilities/dataparsers.cpp; do
  o=$(basename "$f" .cpp).o
  [ -f "$o" ] || g++ -std=c++17 -O1 -I"$QL" -I"$BOOST" -c "$f" -o "$o"
done
g++ -std=c++17 -O1 -I"$QL" -I"$BOOST" -c "$ROOT/tools/golden/p1_all_calendars.cpp" -o driver.o
g++ ./*.o -o p1_all_calendars
cd "$ROOT"
"$BUILD/p1_all_calendars" tests/golden
