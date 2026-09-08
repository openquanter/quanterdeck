#!/bin/bash
# A stand-in for the 1.x manager.sh: enough shape for the adapter tests.
SERVICE="$1"
ACTION="$2"
case "$ACTION" in
  status)  echo "服务 ${SERVICE} 运行中, PID: 4242"; exit 0 ;;
  restart) echo "已向 ${SERVICE} 发送重启信号 (SIGUSR1)"; exit 0 ;;
  start)   echo "已启动 ${SERVICE}"; exit 0 ;;
  stop)    echo "已停止 ${SERVICE}"; exit 0 ;;
  *)       echo "unknown action: ${ACTION}" >&2; exit 2 ;;
esac
