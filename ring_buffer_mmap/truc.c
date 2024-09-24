
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

// page size
#define SIZE 4096

int main(void) {
  int err;

  int fd1 = shm_open("/foo.shm", O_RDWR | O_CREAT, 0666);
  if (fd1 < 0) {
    printf("oh no 1 %s\n", strerror(errno));
    exit(1);
  }

  if ((err = ftruncate(fd1, SIZE)) < 0) {
    printf("oh no ftruncate %s\n", strerror(errno));
    exit(1);
  }

  int fd2 = shm_open("/foo.shm", O_RDONLY, 0666);
  if (fd2 < 0) {
    printf("oh no 2 %s\n", strerror(errno));
    exit(1);
  }

  int fd3 = shm_open("/foo.shm", O_RDWR | O_CREAT, 0666);
  if (fd3 < 0) {
    printf("oh no 3 %s\n", strerror(errno));
    exit(1);
  }

  char *m1 = mmap(0, SIZE, PROT_WRITE, O_RDWR | MAP_SHARED, fd1, 0);
  if (m1 == MAP_FAILED) {
    printf("oh no m1 %s\n", strerror(errno));
    exit(1);
  }

  // reset whole buffer
  memset(m1, 0, SIZE);

  // map data again but just after m1
  char *m3 = mmap(m1 + SIZE, SIZE, PROT_WRITE, O_RDWR | MAP_SHARED | MAP_FIXED,
                  fd3, 0);
  if (m3 == MAP_FAILED) {
    printf("oh no m3 %s\n", strerror(errno));
    exit(1);
  }

  char *m2 = mmap(0, SIZE, PROT_READ, O_RDONLY | MAP_SHARED, fd2, 0);
  if (m2 == MAP_FAILED) {
    printf("oh no m2 %s\n", strerror(errno));
    exit(1);
  }

  char yolo[SIZE * 2 + 1];
  strcpy(m1, "hello world");

  for (int i=0; i < 20; ++i)
    printf ("m2[%d] = %c\n", i, m2[i]);
  memcpy(yolo, m2, 10);
  yolo[SIZE*2] = 0;

  printf("read: %s\n", yolo);

  printf("now write near the end\n");
  strcpy(m1 + SIZE - 10, "this is longer than 10 char");

  memcpy(yolo, m2, SIZE);
  yolo[SIZE * 2] = 0;

  close(fd1);
  close(fd2);
  close(fd3);
  return 0;
}
