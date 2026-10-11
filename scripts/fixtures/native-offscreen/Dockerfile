# SPDX-License-Identifier: Apache-2.0
FROM archlinux:base
RUN pacman -Syu --noconfirm --needed xorg-server-xvfb xorg-xauth webkit2gtk-4.1 gnome-keyring libsecret python ttf-dejavu && pacman -Scc --noconfirm
RUN useradd -m -u 1000 qualification
USER qualification
ENV GDK_BACKEND=x11
ENV LIBGL_ALWAYS_SOFTWARE=1
ENV WEBKIT_DISABLE_DMABUF_RENDERER=1
