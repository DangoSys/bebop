CC = riscv64-linux-gnu-gcc
CFLAGS = -Wall -O2
LDFLAGS = -static
SRCS = spmm.c comp.c sp_matrix.c
OBJS = $(SRCS:.c=.o)
TARGET = spmm

.PHONY: all clean

all: $(TARGET)

$(TARGET): $(OBJS)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $^

%.o: %.c
	$(CC) $(CFLAGS) -c -o $@ $<

clean:
	rm -f $(OBJS) $(TARGET)
