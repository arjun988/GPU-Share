/* GPUMesh R3 cudart stub sample.
 *
 * Host:   gpumesh cuda share && gpumesh cuda allow <client>
 * Client: gpumesh cuda bridge --peer <host>
 * Build stub: cargo build -p gpumesh-cudart-stub
 * Then:
 *   export GPUMESH_CUDA_BRIDGE=127.0.0.1:17999
 *   gcc -O2 examples/cuda_stub_sample.c -L target/debug -lcudart -o /tmp/gm-cuda-sample
 *   LD_LIBRARY_PATH=target/debug /tmp/gm-cuda-sample
 */

#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

extern int cudaGetDeviceCount(int *count);
extern int cudaMalloc(void **p, size_t n);
extern int cudaFree(void *p);
extern int cudaMemcpy(void *dst, const void *src, size_t n, int kind);
extern int cudaDeviceSynchronize(void);
extern int gpumeshVectorAddF32(void *a, void *b, void *out, unsigned n);

#define HTOD 1
#define DTOH 2

int main(void) {
    int n_dev = 0;
    if (cudaGetDeviceCount(&n_dev) != 0) {
        fprintf(stderr, "cudaGetDeviceCount failed (is the bridge up?)\n");
        return 1;
    }
    printf("devices via remoting: %d\n", n_dev);

    const unsigned n = 8;
    float ha[8], hb[8], hout[8];
    for (unsigned i = 0; i < n; i++) {
        ha[i] = (float)i;
        hb[i] = (float)(i * 10);
    }

    void *a = NULL, *b = NULL, *out = NULL;
    if (cudaMalloc(&a, sizeof(ha)) || cudaMalloc(&b, sizeof(hb)) || cudaMalloc(&out, sizeof(hout))) {
        fprintf(stderr, "cudaMalloc failed\n");
        return 1;
    }
    cudaMemcpy(a, ha, sizeof(ha), HTOD);
    cudaMemcpy(b, hb, sizeof(hb), HTOD);
    if (gpumeshVectorAddF32(a, b, out, n) != 0) {
        fprintf(stderr, "vector_add failed\n");
        return 1;
    }
    cudaDeviceSynchronize();
    cudaMemcpy(hout, out, sizeof(hout), DTOH);
    printf("out[3] = %f (expect 33)\n", hout[3]);
    cudaFree(a);
    cudaFree(b);
    cudaFree(out);
    return (hout[3] > 32.5f && hout[3] < 33.5f) ? 0 : 2;
}
