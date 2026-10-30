defmodule NativeWasmComponents.GeneratePdfTest do
  use ExUnit.Case, async: true

  @component_path "functions/generate-pdf/1.0/generate_pdf.wasm"
  @interface {"betty-blocks:generate-pdf/generate-pdf@1.0.0", "generate-pdf"}
  @upload_interface "betty-blocks-types:upload-file/upload-file@4.0.0"

  @stylesheet "@font-face { font-family: Brand; src: url(../fonts/brand.ttf); }\nbody { font-family: Brand; }"
  @font File.read!("functions/generate-pdf/1.0/fonts/LiberationSerif-BoldItalic.ttf")
  @png File.read!("test/fixtures/pixel.png")

  defp helper_context do
    %{
      "application-id" => "06caae5da8234837a330c14a7350ed75",
      "action-id" => "",
      "log-id" => "",
      "encrypted-configurations" => :none,
      "jwt" => :none
    }
  end

  # The upload-file base component is a host import: capture what the function hands it and
  # answer like the real thing would.
  defp upload_import(test_pid) do
    %{
      @upload_interface => %{
        "upload" =>
          {:fn,
           fn _helper_context, input ->
             send(test_pid, {:uploaded, input})
             {:ok, %{"reference" => "file-ref-1", "file-size" => 1, "message" => :none}}
           end}
      }
    }
  end

  defp generate(html, variables \\ :none, file_name \\ "invoice") do
    TestHelper.run_component(
      @component_path,
      @interface,
      [
        helper_context(),
        html,
        variables,
        %{"name" => "Invoice"},
        [%{"name" => "file"}],
        file_name
      ],
      upload_import(self())
    )
  end

  defp uploaded_pdf do
    assert_receive {:uploaded, input}, 5_000
    pdf = Base.decode64!(input["file-base64"])
    assert String.starts_with?(pdf, "%PDF-")
    {input, pdf}
  end

  # Serves the stylesheet, font and image the test documents link to, and tells the test
  # which paths were requested.
  defp serve_assets(sham, test_pid) do
    Sham.stub(sham, fn conn ->
      send(test_pid, {:requested, conn.request_path})

      case conn.request_path do
        "/css/site.css" -> Plug.Conn.send_resp(conn, 200, @stylesheet)
        "/fonts/brand.ttf" -> Plug.Conn.send_resp(conn, 200, @font)
        "/img/logo.png" -> Plug.Conn.send_resp(conn, 200, @png)
        _ -> Plug.Conn.send_resp(conn, 404, "not found")
      end
    end)
  end

  describe "generate-pdf component" do
    setup do
      sham = Sham.start()
      serve_assets(sham, self())
      {:ok, %{host: "http://localhost:#{sham.port}"}}
    end

    test "renders the template, embeds downloaded resources and uploads the PDF", %{host: host} do
      html = """
      <html><head><link rel="stylesheet" href="#{host}/css/site.css"></head>
      <body><p>Invoice {{ invoice.number }}</p><img src="#{host}/img/logo.png" width="40" height="40"></body></html>
      """

      variables = {:some, [%{"key" => "invoice", "value" => {:some, ~s({"number":"INV-7"})}}]}

      assert {:ok, "file-ref-1"} == generate(html, variables)

      {input, pdf} = uploaded_pdf()
      assert input["full-filename"] == "invoice.pdf"
      assert input["model"] == %{name: "Invoice"}
      assert input[:property] == %{name: "file"}

      assert_receive {:requested, "/css/site.css"}
      assert_receive {:requested, "/fonts/brand.ttf"}
      assert_receive {:requested, "/img/logo.png"}

      assert pdf =~ "+LiberationSerif-BoldItalic", "downloaded font should be embedded"
      assert pdf =~ "/Subtype /Image", "downloaded image should be drawn"
    end

    test "still produces a PDF when resources fail to download", %{host: host} do
      html = """
      <html><head><link rel="stylesheet" href="#{host}/css/missing.css"></head>
      <body><p>Still here</p><img src="#{host}/img/missing.png"></body></html>
      """

      assert {:ok, "file-ref-1"} == generate(html)
      {_input, _pdf} = uploaded_pdf()

      assert_receive {:requested, "/css/missing.css"}
      assert_receive {:requested, "/img/missing.png"}
    end

    test "rejects an empty file name without rendering" do
      assert {:error, "File name is required"} == generate("<p>x</p>", :none, " ")
      refute_receive {:uploaded, _}
    end
  end
end
