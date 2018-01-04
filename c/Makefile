libdeps :=  $(wildcard ../target/release/*.so) $(wildcard ../target/release/*.dylib) $(wildcard ../target/release/*.dll)
ffi_example: ffi.h vec_u8.h custom_alloc.h $(libdeps)
	gcc -g -o ffi_example ffi.c -L../target/release -ldivans -Wl,-rpath -Wl,../target/release
clean:
	rm -f ffi_example
