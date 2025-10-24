import React from "react";
import ReactDOM from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import "./styles.css";
import App from "./App";
import Home from "./pages/Home";
import Block from "./pages/Block";
import Tx from "./pages/Tx";
import KeyImage from "./pages/KeyImage";
import Stats from "./pages/Stats";
import NotFound from "./pages/NotFound";

const router = createBrowserRouter([
  {
    path: "/",
    element: <App />,
    errorElement: <NotFound />,
    children: [
      { index: true, element: <Home /> },
      { path: "block/:id", element: <Block /> },
      { path: "tx/:hash", element: <Tx /> },
      { path: "key_image/:hex", element: <KeyImage /> },
      { path: "stats", element: <Stats /> },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);
