#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

void bubble_sort(int *a, unsigned int size) {
  bool swaped = false;
  int temp;

  for (int i = 0; i < size; ++i) {
    swaped = false;
    for (int j = 0; j < size - 1; ++j) {
      if (a[j] > a[j + 1]) {
        swaped = true;
        temp = a[j];
        a[j] = a[j + 1];
        a[j + 1] = temp;
      }
    }

    if (!swaped)
      return;
  }
}

int main(void) {
  int n;

  scanf("%d", &n);
  int *arr = malloc(n * sizeof(int));

  for (int i = 0; i < n; ++i) {
    scanf("%d", arr + i);
  }

  bubble_sort(arr, n);

  for (int i = 0; i < n; ++i) {
    printf("%d ", arr[i]);
  }
}
