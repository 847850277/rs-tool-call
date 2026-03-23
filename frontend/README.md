# Frontend Demo

这个目录是一个独立的前端示例项目，用来演示：

1. 填写 OSS 或 CDN 语音文件 URL，调用后端 `/translate/media` 转成文字
2. 调用后端 `/extract/form` 接口把文字自动填充到表单

## 运行方式

这是一个纯静态前端，不依赖打包器。任选一种方式启动本地静态服务：

```bash
cd frontend
python3 -m http.server 5500
```

然后打开：

```text
http://127.0.0.1:5500
```

## 使用说明

1. 确认后端服务已启动，默认地址是 `http://127.0.0.1:7878`
2. 填写可访问的语音文件 URL
3. 点击“转文字”
4. 点击“自动填充”，前端会调用 `/extract/form`
5. 右侧表单会自动写入抽取结果，并显示缺失字段、校验问题和接口返回

## 当前限制

- 第一步依赖后端 `/translate/media` 已可正常访问阿里百炼媒体翻译接口
- 当前前端只接收远程语音文件 URL，不做本地文件上传
- 当前示例表单固定为 `basic_profile`
- 结构化抽取依赖后端已配置好可用的 LLM


## 测试语音文件
https://filecdn.ailecheng.com/20260323/3ac3d960734e946fba2542b6310be0bd.wav
