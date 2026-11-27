# /init for the Linux guest in Iskra's hypervisor test: log a greeting, then power off.
# Writes to /dev/kmsg so the message goes out through the kernel console (polled serial),
# which needs no serial interrupts from the VMM.
    .intel_syntax noprefix
    .globl _start
_start:
    mov eax, 2                      # open("/dev/kmsg", O_WRONLY)
    lea rdi, [rip + kmsg]
    mov esi, 1
    xor edx, edx
    syscall
    mov edi, eax                    # fd
    mov eax, 1                      # write(fd, msg, len)
    lea rsi, [rip + msg]
    mov edx, OFFSET msg_len           # OFFSET: the constant, not memory at that address
    syscall
    mov eax, 169                    # reboot(LINUX_REBOOT_MAGIC1, MAGIC2, POWER_OFF)
    mov edi, 0xfee1dead
    mov esi, 0x28121969
    mov edx, 0x4321fedc
    syscall
1:  jmp 1b

kmsg: .asciz "/dev/kmsg"
msg:  .ascii "*** Hello from Linux, running in a VM inside Iskra! ***\n"
msg_len = . - msg
