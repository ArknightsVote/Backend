package handlers

import (
	"bytes"
	"os"
	"runtime"
	"runtime/pprof"

	"github.com/gofiber/fiber/v2"
)

type SystemHandle struct{}

func RegisterSystem(system fiber.Router) {
	var handler SystemHandle

	system.Use(handler.Verify)

	system.Get("/info", handler.GetServerInfo)
	system.Post("/clean", handler.TriggerGC)
	system.Post("/stack", handler.GetStackInfo)
}

// GetServerInfo 获取服务器信息
func (s *SystemHandle) GetServerInfo(ctx *fiber.Ctx) error {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	serverInfo := map[string]interface{}{
		"go_version":  runtime.Version(),
		"cpu_num":     runtime.NumCPU(),
		"goroutines":  runtime.NumGoroutine(),
		"mem_alloc":   m.Alloc,
		"heap_alloc":  m.HeapAlloc,
		"total_alloc": m.TotalAlloc,
		"sys":         m.Sys,
	}

	return ctx.JSON(fiber.Map{
		"code": "200",
		"data": serverInfo,
	})
}

// TriggerGC 垃圾主动回收
func (s *SystemHandle) TriggerGC(ctx *fiber.Ctx) error {
	runtime.GC()

	return ctx.JSON(fiber.Map{
		"code":    "200",
		"message": "ok",
	})
}

// GetStackInfo 获取堆栈信息
func (s *SystemHandle) GetStackInfo(ctx *fiber.Ctx) error {
	var buf bytes.Buffer
	pprof.Lookup("goroutine").WriteTo(&buf, 1)

	return ctx.JSON(fiber.Map{
		"code": "200",
		"data": buf.String(),
	})
}

// Verify 顶针身份
func (s *SystemHandle) Verify(c *fiber.Ctx) error {
	appSystemKey := os.Getenv("APP_SYSTEM_KEY")
	if appSystemKey == "" {
		return c.Status(fiber.StatusInternalServerError).JSON(fiber.Map{
			"error": "APP_SYSTEM_KEY is not set",
		})
	}

	requestKey := c.Query("key")
	if requestKey == "" || requestKey != appSystemKey {
		return c.Status(fiber.StatusUnauthorized).JSON(fiber.Map{
			"error": "invalid key",
		})
	}

	return c.Next()
}
