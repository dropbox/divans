libdeps :=  $(wildcard ../target/release/*.so) $(wildcard ../target/release/*.dylib) $(wildcard ../target/release/*.dll)
ffi_example: example.c arg.h divans/ffi.h vec_u8.h custom_alloc.h $(libdeps)
	gcc -Wall -g -O2 -o ffi_example example.c -L../target/release -ldivans -Wl,-rpath -Wl,../target/release
debug: example.c arg.h divans/ffi.h vec_u8.h custom_alloc.h $(libdeps)
	gcc -Wall -Wno-unused-result -g -o ffi_example_d example.c -L../target/debug -ldivans -Wl,-rpath -Wl,../target/debug
clean:
	rm -f ffi_example_d ffi_example
