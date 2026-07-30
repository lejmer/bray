.text
.global _start
.type _start, @function
_start:
    mov $42, %edi
    call bray_identity
    mov %eax, %edi
    mov $60, %eax
    syscall
